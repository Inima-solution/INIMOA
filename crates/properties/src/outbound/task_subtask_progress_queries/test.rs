//! Database contract tests for canonical direct-subtask progress.

use sqlx::{Pool, Postgres};
use system_properties::{StatusOption, SystemPropertyKey};
use uuid::Uuid;

use super::get_task_subtask_progress;
use macro_db_migrator::MACRO_DB_MIGRATIONS;

const OWNER: &str = "task-dependencies-owner";
const PROJECT: &str = "task-dependencies-project-a";

async fn task(pool: &Pool<Postgres>, id: Uuid, project: Option<&str>, is_task: bool) {
    sqlx::query("INSERT INTO \"Document\" (id, name, owner, \"projectId\") VALUES ($1, 'subtask progress', $2, $3)")
        .bind(id.to_string()).bind(OWNER).bind(project).execute(pool).await.unwrap();
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
        .bind(Uuid::new_v4()).bind(id.to_string()).bind(definition).bind(value).execute(pool).await.unwrap();
}

fn references(ids: &[Uuid]) -> serde_json::Value {
    serde_json::json!({"type":"EntityReference","value":ids.iter().map(|id| serde_json::json!({"entity_type":"TASK","entity_id":id})).collect::<Vec<_>>()})
}

async fn canonical_edge(pool: &Pool<Postgres>, parent: Uuid, child: Uuid) {
    property(
        pool,
        child,
        SystemPropertyKey::PARENT_TASK_UUID,
        references(&[parent]),
    )
    .await;
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn subtask_progress_counts_only_live_canonical_children(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::from_u128(0xE501);
    let completed = Uuid::from_u128(0xE502);
    let incomplete = Uuid::from_u128(0xE503);
    let canceled = Uuid::from_u128(0xE504);
    for id in [source, completed, incomplete, canceled] {
        task(&pool, id, None, true).await;
    }
    property(
        &pool,
        source,
        SystemPropertyKey::SUBTASKS_UUID,
        references(&[completed, incomplete, canceled]),
    )
    .await;
    for child in [completed, incomplete, canceled] {
        canonical_edge(&pool, source, child).await;
    }
    property(
        &pool,
        completed,
        SystemPropertyKey::STATUS_UUID,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]}),
    )
    .await;
    property(
        &pool,
        canceled,
        SystemPropertyKey::STATUS_UUID,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::CANCELED_UUID]}),
    )
    .await;

    let rows = get_task_subtask_progress(&pool, &[source]).await?.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].task_id, source);
    assert_eq!(rows[0].subtask_ids, vec![completed, incomplete, canceled]);
    assert_eq!(rows[0].completed_subtask_ids, vec![completed]);
    assert_eq!(rows[0].canceled_subtask_ids, vec![canceled]);
    assert!(!rows[0].has_unavailable_subtasks);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn subtask_progress_malformed_or_noncanonical_edges_fail_closed_without_ids(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let malformed = Uuid::from_u128(0xE511);
    let one_sided = Uuid::from_u128(0xE512);
    let duplicate = Uuid::from_u128(0xE513);
    let child = Uuid::from_u128(0xE514);
    let deleted = Uuid::from_u128(0xE515);
    let non_task = Uuid::from_u128(0xE516);
    let cross_project = Uuid::from_u128(0xE517);
    let deleted_source = Uuid::from_u128(0xE518);
    let non_task_source = Uuid::from_u128(0xE519);
    let cross_source = Uuid::from_u128(0xE51A);
    for id in [
        malformed,
        one_sided,
        duplicate,
        child,
        deleted,
        deleted_source,
        non_task_source,
        cross_source,
    ] {
        task(&pool, id, Some(PROJECT), true).await;
    }
    task(&pool, non_task, Some(PROJECT), false).await;
    task(
        &pool,
        cross_project,
        Some("task-dependencies-project-b"),
        true,
    )
    .await;
    property(
        &pool,
        malformed,
        SystemPropertyKey::SUBTASKS_UUID,
        serde_json::json!({"bad":true}),
    )
    .await;
    property(
        &pool,
        one_sided,
        SystemPropertyKey::SUBTASKS_UUID,
        references(&[child]),
    )
    .await;
    property(
        &pool,
        duplicate,
        SystemPropertyKey::SUBTASKS_UUID,
        references(&[child, child]),
    )
    .await;
    property(
        &pool,
        deleted_source,
        SystemPropertyKey::SUBTASKS_UUID,
        references(&[deleted]),
    )
    .await;
    property(
        &pool,
        non_task_source,
        SystemPropertyKey::SUBTASKS_UUID,
        references(&[non_task]),
    )
    .await;
    property(
        &pool,
        cross_source,
        SystemPropertyKey::SUBTASKS_UUID,
        references(&[cross_project]),
    )
    .await;
    property(
        &pool,
        child,
        SystemPropertyKey::PARENT_TASK_UUID,
        references(&[duplicate]),
    )
    .await;
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\" = NOW() WHERE id = $1")
        .bind(deleted.to_string())
        .execute(&pool)
        .await?;
    let rows = get_task_subtask_progress(
        &pool,
        &[
            malformed,
            one_sided,
            duplicate,
            deleted_source,
            non_task_source,
            cross_source,
        ],
    )
    .await?
    .unwrap();
    for row in rows {
        assert!(row.subtask_ids.is_empty());
        assert!(row.completed_subtask_ids.is_empty());
        assert!(row.canceled_subtask_ids.is_empty());
        assert!(row.has_unavailable_subtasks);
    }
    assert!(
        get_task_subtask_progress(&pool, &[deleted])
            .await?
            .is_none()
    );
    assert!(
        get_task_subtask_progress(&pool, &[Uuid::from_u128(0xE51B)])
            .await?
            .is_none()
    );
    assert!(
        get_task_subtask_progress(&pool, &[non_task])
            .await?
            .is_none()
    );
    assert!(
        get_task_subtask_progress(&pool, &[cross_project])
            .await?
            .is_some()
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn subtask_progress_preserves_batch_order_and_duplicates(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let first = Uuid::from_u128(0xE521);
    let second = Uuid::from_u128(0xE522);
    task(&pool, first, Some(PROJECT), true).await;
    task(&pool, second, Some(PROJECT), true).await;
    let rows = get_task_subtask_progress(&pool, &[second, first, second])
        .await?
        .unwrap();
    assert_eq!(
        rows.iter().map(|row| row.task_id).collect::<Vec<_>>(),
        vec![second, first, second]
    );
    assert!(
        rows.iter()
            .all(|row| row.subtask_ids.is_empty() && !row.has_unavailable_subtasks)
    );
    Ok(())
}
