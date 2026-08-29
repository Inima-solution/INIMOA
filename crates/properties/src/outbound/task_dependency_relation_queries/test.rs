//! Database contract tests for canonical direct dependency relations.

use sqlx::{Pool, Postgres};
use system_properties::{StatusOption, SystemPropertyKey};
use uuid::Uuid;

use super::get_task_dependency_relations;
use crate::domain::model::TaskReadiness;
use macro_db_migrator::MACRO_DB_MIGRATIONS;

const OWNER: &str = "task-dependencies-owner";
const PROJECT_A: &str = "task-dependencies-project-a";
const PROJECT_B: &str = "task-dependencies-project-b";

async fn task(pool: &Pool<Postgres>, id: Uuid, project: Option<&str>, is_task: bool) {
    sqlx::query("INSERT INTO \"Document\" (id, name, owner, \"projectId\") VALUES ($1, 'dependency relation', $2, $3)")
        .bind(id.to_string())
        .bind(OWNER)
        .bind(project)
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

async fn property(pool: &Pool<Postgres>, id: Uuid, definition: Uuid, value: serde_json::Value) {
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
        .bind(Uuid::new_v4())
        .bind(id.to_string())
        .bind(definition)
        .bind(value)
        .execute(pool)
        .await
        .unwrap();
}

fn references(ids: impl IntoIterator<Item = Uuid>) -> serde_json::Value {
    serde_json::json!({"type":"EntityReference","value": ids.into_iter().map(|id| serde_json::json!({"entity_type":"TASK","entity_id":id})).collect::<Vec<_>>()})
}

async fn completed(pool: &Pool<Postgres>, id: Uuid) {
    property(
        pool,
        id,
        SystemPropertyKey::STATUS_UUID,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]}),
    )
    .await;
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn dependency_relations_preserve_empty_personal_sources_forward_order_and_uuid_successor_order(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    assert_eq!(
        get_task_dependency_relations(&pool, &[]).await?,
        Some(vec![])
    );

    let source = Uuid::from_u128(0xE901);
    let first = Uuid::from_u128(0xE902);
    let second = Uuid::from_u128(0xE903);
    let successor_first = Uuid::from_u128(0xE904);
    let successor_second = Uuid::from_u128(0xE905);
    for id in [source, first, second, successor_first, successor_second] {
        task(&pool, id, None, true).await;
    }
    property(
        &pool,
        source,
        SystemPropertyKey::DEPENDS_ON_UUID,
        references([second, first]),
    )
    .await;
    completed(&pool, first).await;
    completed(&pool, second).await;
    for id in [successor_second, successor_first] {
        property(
            &pool,
            id,
            SystemPropertyKey::DEPENDS_ON_UUID,
            references([source]),
        )
        .await;
    }
    let rows = get_task_dependency_relations(&pool, &[source, source])
        .await?
        .unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row.readiness.task_id, source);
        assert_eq!(row.readiness.readiness, TaskReadiness::Ready);
        assert_eq!(row.readiness.depends_on_task_ids, vec![second, first]);
        assert!(row.readiness.blocking_task_ids.is_empty());
        assert!(!row.readiness.has_unavailable_dependencies);
        assert_eq!(
            row.successor_task_ids,
            vec![successor_first, successor_second]
        );
        assert!(!row.has_unavailable_successors);
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn dependency_relations_reject_invalid_source_batches_and_redact_invalid_forward_and_reverse_mentions(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::from_u128(0xE911);
    let completed_task = Uuid::from_u128(0xE912);
    let deleted = Uuid::from_u128(0xE913);
    let non_task = Uuid::from_u128(0xE914);
    let cross_project = Uuid::from_u128(0xE915);
    let malformed_successor = Uuid::from_u128(0xE916);
    let valid_successor = Uuid::from_u128(0xE917);
    let deleted_successor = Uuid::from_u128(0xE919);
    let non_task_successor = Uuid::from_u128(0xE91A);
    let cross_project_successor = Uuid::from_u128(0xE91B);
    for id in [
        source,
        completed_task,
        deleted,
        malformed_successor,
        valid_successor,
        deleted_successor,
    ] {
        task(&pool, id, Some(PROJECT_A), true).await;
    }
    task(&pool, non_task, Some(PROJECT_A), false).await;
    task(&pool, non_task_successor, Some(PROJECT_A), false).await;
    task(&pool, cross_project, Some(PROJECT_B), true).await;
    task(&pool, cross_project_successor, Some(PROJECT_B), true).await;
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\" = NOW() WHERE id = $1")
        .bind(deleted.to_string())
        .execute(&pool)
        .await?;
    completed(&pool, completed_task).await;
    property(&pool, source, SystemPropertyKey::DEPENDS_ON_UUID, serde_json::json!({"type":"EntityReference","value":[
        {"entity_type":"TASK","entity_id":completed_task}, {"entity_type":"TASK","entity_id":completed_task}, {"entity_type":"TASK","entity_id":deleted},
        {"entity_type":"TASK","entity_id":non_task}, {"entity_type":"TASK","entity_id":cross_project},
        {"entity_type":"TASK","entity_id":source}, {"entity_type":"TASK","entity_id":"not-a-uuid"}
    ]})).await;
    property(
        &pool,
        valid_successor,
        SystemPropertyKey::DEPENDS_ON_UUID,
        references([source]),
    )
    .await;
    for id in [
        deleted_successor,
        non_task_successor,
        cross_project_successor,
    ] {
        property(
            &pool,
            id,
            SystemPropertyKey::DEPENDS_ON_UUID,
            references([source]),
        )
        .await;
    }
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\" = NOW() WHERE id = $1")
        .bind(deleted_successor.to_string())
        .execute(&pool)
        .await?;
    property(&pool, malformed_successor, SystemPropertyKey::DEPENDS_ON_UUID, serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":source},{"entity_type":"TASK","entity_id":source}]})).await;

    assert_eq!(
        get_task_dependency_relations(&pool, &[Uuid::from_u128(0xE918), source]).await?,
        None,
        "a missing source rejects the entire requested batch"
    );
    assert_eq!(
        get_task_dependency_relations(&pool, &[deleted, source]).await?,
        None,
        "a deleted source rejects the entire requested batch"
    );
    assert_eq!(
        get_task_dependency_relations(&pool, &[non_task, source]).await?,
        None,
        "a live non-Task source rejects the entire requested batch"
    );
    let rows = get_task_dependency_relations(&pool, &[source])
        .await?
        .unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.readiness.task_id, source);
    assert_eq!(row.readiness.readiness, TaskReadiness::Blocked);
    assert_eq!(row.readiness.depends_on_task_ids, vec![completed_task]);
    assert!(row.readiness.blocking_task_ids.is_empty());
    assert!(row.readiness.has_unavailable_dependencies);
    assert_eq!(row.successor_task_ids, vec![valid_successor]);
    assert!(row.has_unavailable_successors);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn dependency_relations_cap_more_than_200_successors_without_disclosing_a_count(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::from_u128(0xE921);
    task(&pool, source, Some(PROJECT_A), true).await;
    for ordinal in 0..201_u128 {
        let successor = Uuid::from_u128(0xEA000 + ordinal);
        task(&pool, successor, Some(PROJECT_A), true).await;
        property(
            &pool,
            successor,
            SystemPropertyKey::DEPENDS_ON_UUID,
            references([source]),
        )
        .await;
    }
    let row = get_task_dependency_relations(&pool, &[source])
        .await?
        .unwrap()
        .pop()
        .unwrap();
    assert!(row.successor_task_ids.is_empty());
    assert!(row.has_unavailable_successors);
    Ok(())
}
