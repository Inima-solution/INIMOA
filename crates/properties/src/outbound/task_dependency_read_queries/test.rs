//! Complete database-source matrix for task dependency readiness. It is not
//! run in this source-only checkpoint; the later SQLx gate executes it and
//! generates the one approved query cache artifact.

use sqlx::{Pool, Postgres};
use uuid::Uuid;

use super::get_task_dependency_readiness;
use crate::domain::model::TaskReadiness;
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use system_properties::{StatusOption, SystemPropertyKey};

const OWNER: &str = "task-dependencies-owner";

async fn scoped_project(pool: &Pool<Postgres>, team_id: Uuid) {
    sqlx::query(
        "INSERT INTO team (id, name, owner_id) VALUES ($1, 'task dependency readiness', $2)",
    )
    .bind(team_id)
    .bind(OWNER)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO team_user (user_id, team_id, team_role) VALUES ($1, $2, 'owner')")
        .bind(OWNER)
        .bind(team_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn task(pool: &Pool<Postgres>, id: Uuid, project_id: &str, is_task: bool) {
    sqlx::query(
        "INSERT INTO \"Document\" (id, name, owner, \"projectId\") VALUES ($1, 'task', $2, $3)",
    )
    .bind(id.to_string())
    .bind(OWNER)
    .bind(project_id)
    .execute(pool)
    .await
    .unwrap();
    if is_task {
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
            .bind(id.to_string())
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn raw_property(
    pool: &Pool<Postgres>,
    task_id: Uuid,
    definition_id: Uuid,
    value: serde_json::Value,
) {
    sqlx::query(
        r#"
        INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values)
        VALUES ($1, $2, 'TASK', $3, $4)
        ON CONFLICT (entity_id, entity_type, property_definition_id)
        DO UPDATE SET values = EXCLUDED.values
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(task_id.to_string())
    .bind(definition_id)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

async fn null_property(pool: &Pool<Postgres>, task_id: Uuid, definition_id: Uuid) {
    sqlx::query(
        "INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, NULL)",
    )
    .bind(Uuid::new_v4())
    .bind(task_id.to_string())
    .bind(definition_id)
    .execute(pool)
    .await
    .unwrap();
}

fn depends_on(ids: impl IntoIterator<Item = Uuid>) -> serde_json::Value {
    serde_json::json!({"type": "EntityReference", "value": ids.into_iter().map(|id| {
        serde_json::json!({"entity_type": "TASK", "entity_id": id})
    }).collect::<Vec<_>>()})
}

async fn status(pool: &Pool<Postgres>, task_id: Uuid, value: serde_json::Value) {
    raw_property(pool, task_id, SystemPropertyKey::STATUS_UUID, value).await;
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn readiness_db_matrix_direct_status_unavailable_and_ordering(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let team_id = Uuid::from_u128(0xD500);
    let source = Uuid::from_u128(0xD501);
    let completed = Uuid::from_u128(0xD502);
    let canceled = Uuid::from_u128(0xD503);
    let missing = Uuid::from_u128(0xD504);
    scoped_project(&pool, team_id).await;
    for id in [source, completed, canceled] {
        task(&pool, id, "task-dependencies-project-a", true).await;
    }
    raw_property(
        &pool,
        source,
        SystemPropertyKey::DEPENDS_ON_UUID,
        serde_json::json!({
            "type": "EntityReference",
            "value": [
                {"entity_type": "TASK", "entity_id": completed},
                {"entity_type": "TASK", "entity_id": canceled},
                {"entity_type": "TASK", "entity_id": completed},
                {"entity_type": "TASK", "entity_id": missing}
            ]
        }),
    )
    .await;
    raw_property(
        &pool,
        completed,
        SystemPropertyKey::STATUS_UUID,
        serde_json::json!({"type": "SelectOption", "value": [StatusOption::COMPLETED_UUID]}),
    )
    .await;
    raw_property(
        &pool,
        canceled,
        SystemPropertyKey::STATUS_UUID,
        serde_json::json!({"type": "SelectOption", "value": [StatusOption::CANCELED_UUID]}),
    )
    .await;

    let rows = get_task_dependency_readiness(
        &pool,
        "task-dependencies-project-a",
        team_id,
        &[source, source],
    )
    .await?
    .unwrap();
    assert_eq!(rows.len(), 1, "input duplicates collapse");
    assert_eq!(rows[0].readiness, TaskReadiness::Blocked);
    assert_eq!(rows[0].depends_on_task_ids, vec![completed, canceled]);
    assert_eq!(rows[0].blocking_task_ids, vec![canceled]);
    assert!(rows[0].has_unavailable_dependencies);

    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn readiness_db_matrix_all_completed_is_ready(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let team = Uuid::from_u128(0xD505);
    let source = Uuid::from_u128(0xD506);
    let first = Uuid::from_u128(0xD507);
    let second = Uuid::from_u128(0xD508);
    scoped_project(&pool, team).await;
    for id in [source, first, second] {
        task(&pool, id, "task-dependencies-project-a", true).await;
    }
    raw_property(
        &pool,
        source,
        SystemPropertyKey::DEPENDS_ON_UUID,
        depends_on([second, first]),
    )
    .await;
    for id in [first, second] {
        status(
            &pool,
            id,
            serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]}),
        )
        .await;
    }
    let row = get_task_dependency_readiness(&pool, "task-dependencies-project-a", team, &[source])
        .await?
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(row.readiness, TaskReadiness::Ready);
    assert_eq!(row.depends_on_task_ids, vec![second, first]);
    assert!(row.blocking_task_ids.is_empty());
    assert!(!row.has_unavailable_dependencies);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn readiness_db_matrix_malformed_top_level_depends_is_unavailable(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let team = Uuid::from_u128(0xD509);
    let source = Uuid::from_u128(0xD50A);
    scoped_project(&pool, team).await;
    task(&pool, source, "task-dependencies-project-a", true).await;
    raw_property(
        &pool,
        source,
        SystemPropertyKey::DEPENDS_ON_UUID,
        serde_json::json!({"bad": true}),
    )
    .await;
    let row = get_task_dependency_readiness(&pool, "task-dependencies-project-a", team, &[source])
        .await?
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(row.readiness, TaskReadiness::Blocked);
    assert!(row.depends_on_task_ids.is_empty());
    assert!(row.blocking_task_ids.is_empty());
    assert!(row.has_unavailable_dependencies);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn readiness_db_matrix_empty_and_all_status_forms(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let team = Uuid::from_u128(0xD510);
    let no_row = Uuid::from_u128(0xD511);
    let null_depends = Uuid::from_u128(0xD512);
    let source = Uuid::from_u128(0xD513);
    let ids = [
        Uuid::from_u128(0xD514),
        Uuid::from_u128(0xD515),
        Uuid::from_u128(0xD516),
        Uuid::from_u128(0xD517),
        Uuid::from_u128(0xD518),
        Uuid::from_u128(0xD519),
        Uuid::from_u128(0xD51A),
        Uuid::from_u128(0xD51B),
        Uuid::from_u128(0xD51C),
    ];
    scoped_project(&pool, team).await;
    for id in [no_row, null_depends, source].into_iter().chain(ids) {
        task(&pool, id, "task-dependencies-project-a", true).await;
    }
    null_property(&pool, null_depends, SystemPropertyKey::DEPENDS_ON_UUID).await;
    raw_property(
        &pool,
        source,
        SystemPropertyKey::DEPENDS_ON_UUID,
        depends_on(ids),
    )
    .await;
    for (id, option) in ids[..4].iter().zip([
        StatusOption::NOT_STARTED_UUID,
        StatusOption::IN_PROGRESS_UUID,
        StatusOption::IN_REVIEW_UUID,
        StatusOption::CANCELED_UUID,
    ]) {
        status(
            &pool,
            *id,
            serde_json::json!({"type":"SelectOption","value":[option]}),
        )
        .await;
    }
    // absent, SQL NULL, malformed, unknown, and multiple values all block.
    null_property(&pool, ids[5], SystemPropertyKey::STATUS_UUID).await;
    status(&pool, ids[6], serde_json::json!({"bad":true})).await;
    status(
        &pool,
        ids[7],
        serde_json::json!({"type":"SelectOption","value":[Uuid::new_v4()]}),
    )
    .await;
    status(&pool, ids[8], serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID, StatusOption::IN_PROGRESS_UUID]})).await;
    let rows = get_task_dependency_readiness(
        &pool,
        "task-dependencies-project-a",
        team,
        &[no_row, null_depends, source],
    )
    .await?
    .unwrap();
    assert_eq!(rows[0].readiness, TaskReadiness::Ready);
    assert_eq!(rows[1].readiness, TaskReadiness::Ready);
    assert_eq!(rows[2].readiness, TaskReadiness::Blocked);
    assert_eq!(rows[2].blocking_task_ids, ids);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn readiness_db_matrix_unavailable_refs_do_not_leak(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let team = Uuid::from_u128(0xD530);
    let source = Uuid::from_u128(0xD531);
    let completed = Uuid::from_u128(0xD532);
    let deleted = Uuid::from_u128(0xD533);
    let non_task = Uuid::from_u128(0xD534);
    let cross = Uuid::from_u128(0xD535);
    let specific_message_target = Uuid::from_u128(0xD536);
    scoped_project(&pool, team).await;
    for id in [source, completed, deleted] {
        task(&pool, id, "task-dependencies-project-a", true).await;
    }
    task(&pool, non_task, "task-dependencies-project-a", false).await;
    task(&pool, cross, "task-dependencies-project-b", true).await;
    task(
        &pool,
        specific_message_target,
        "task-dependencies-project-a",
        true,
    )
    .await;
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\" = NOW() WHERE id = $1")
        .bind(deleted.to_string())
        .execute(&pool)
        .await?;
    raw_property(&pool, source, SystemPropertyKey::DEPENDS_ON_UUID, serde_json::json!({"type":"EntityReference","value":[
        {"entity_type":"TASK","entity_id":completed}, {"entity_type":"TASK","entity_id":deleted},
        {"entity_type":"TASK","entity_id":non_task}, {"entity_type":"TASK","entity_id":cross},
        {"entity_type":"TASK","entity_id":source},
        {"entity_type":"TASK","entity_id":specific_message_target,"specific_message_id":"00000000-0000-0000-0000-000000000001"},
        {"entity_type":"DOCUMENT","entity_id":Uuid::new_v4()}, {"entity_type":"TASK","entity_id":"not-a-uuid"}
    ]})).await;
    status(
        &pool,
        completed,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]}),
    )
    .await;
    status(
        &pool,
        specific_message_target,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]}),
    )
    .await;
    let rows = get_task_dependency_readiness(
        &pool,
        "task-dependencies-project-a",
        team,
        &[Uuid::new_v4(), deleted, non_task, cross, source],
    )
    .await?
    .unwrap();
    assert_eq!(rows.len(), 1, "invalid source ids omit");
    assert_eq!(rows[0].readiness, TaskReadiness::Blocked);
    assert_eq!(rows[0].depends_on_task_ids, vec![completed]);
    assert!(rows[0].blocking_task_ids.is_empty());
    assert!(rows[0].has_unavailable_dependencies);
    for id in [deleted, non_task, cross, source, specific_message_target] {
        assert!(!rows[0].depends_on_task_ids.contains(&id));
        assert!(!rows[0].blocking_task_ids.contains(&id));
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn readiness_db_matrix_project_witness_and_200_inputs(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let team = Uuid::from_u128(0xD540);
    let source = Uuid::from_u128(0xD541);
    scoped_project(&pool, team).await;
    task(&pool, source, "task-dependencies-project-a", true).await;
    let inputs = std::iter::once(source)
        .chain((0..199).map(|n| Uuid::from_u128(0xD550 + n)))
        .collect::<Vec<_>>();
    assert_eq!(
        get_task_dependency_readiness(&pool, "task-dependencies-project-a", team, &inputs)
            .await?
            .unwrap()
            .len(),
        1
    );
    assert!(
        get_task_dependency_readiness(
            &pool,
            "task-dependencies-project-a",
            Uuid::from_u128(0xD5FF),
            &[source]
        )
        .await?
        .is_none()
    );
    sqlx::query("UPDATE \"Project\" SET \"deletedAt\" = NOW() WHERE id = $1")
        .bind("task-dependencies-project-a")
        .execute(&pool)
        .await?;
    assert!(
        get_task_dependency_readiness(&pool, "task-dependencies-project-a", team, &[source])
            .await?
            .is_none()
    );
    assert!(
        get_task_dependency_readiness(
            &pool,
            "task-dependencies-project-b",
            Uuid::from_u128(0xD5FE),
            &[source]
        )
        .await?
        .is_none()
    );
    Ok(())
}
