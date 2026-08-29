use macro_db_migrator::MACRO_DB_MIGRATIONS;
use models_properties::{EntityReference, EntityType};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use super::final_ready_dependents;
use crate::{StatusOption, SystemPropertyKey};

async fn task(pool: &Pool<Postgres>, id: Uuid, project_id: &str, is_task: bool) {
    sqlx::query("INSERT INTO \"Document\" (id, name, owner, \"projectId\") VALUES ($1, 'task', 'task-dependencies-owner', $2)")
        .bind(id.to_string()).bind(project_id).execute(pool).await.unwrap();
    if is_task {
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
            .bind(id.to_string())
            .execute(pool)
            .await
            .unwrap();
    }
}
async fn property(
    pool: &Pool<Postgres>,
    task_id: Uuid,
    definition: Uuid,
    value: serde_json::Value,
) {
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
        .bind(Uuid::new_v4()).bind(task_id.to_string()).bind(definition).bind(value).execute(pool).await.unwrap();
}
async fn completed(pool: &Pool<Postgres>, task_id: Uuid) {
    property(
        pool,
        task_id,
        SystemPropertyKey::STATUS_UUID,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]}),
    )
    .await;
}
async fn depends(pool: &Pool<Postgres>, task_id: Uuid, refs: serde_json::Value) {
    property(pool, task_id, SystemPropertyKey::DEPENDS_ON_UUID, refs).await;
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(
        path = "../../../../properties/fixtures",
        scripts("task_dependencies_seed")
    )
)]
async fn shared_final_ready_fanout_is_distinct_ordered_excluded_and_final(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let ready_a = Uuid::new_v4();
    let ready_b = Uuid::new_v4();
    let excluded = Uuid::new_v4();
    let incomplete = Uuid::new_v4();
    let deleted = Uuid::new_v4();
    let non_task = Uuid::new_v4();
    let cross_project = Uuid::new_v4();
    let blocked_incomplete = Uuid::new_v4();
    let blocked_deleted = Uuid::new_v4();
    let blocked_non_task = Uuid::new_v4();
    let blocked_cross_project = Uuid::new_v4();
    let malformed = Uuid::new_v4();
    let message_ref = Uuid::new_v4();
    for task_id in [
        first,
        second,
        ready_a,
        ready_b,
        excluded,
        incomplete,
        deleted,
        blocked_incomplete,
        blocked_deleted,
        blocked_non_task,
        blocked_cross_project,
        malformed,
        message_ref,
    ] {
        task(&pool, task_id, "task-dependencies-project-a", true).await;
    }
    task(&pool, non_task, "task-dependencies-project-a", false).await;
    task(&pool, cross_project, "task-dependencies-project-b", true).await;
    for task_id in [
        first,
        second,
        ready_a,
        ready_b,
        excluded,
        deleted,
        cross_project,
    ] {
        completed(&pool, task_id).await;
    }
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\" = NOW() WHERE id = $1")
        .bind(deleted.to_string())
        .execute(&pool)
        .await?;
    let refs = |ids: Vec<Uuid>| serde_json::json!({"type":"EntityReference","value": ids.into_iter().map(|id| EntityReference::new(id.to_string(), EntityType::Task)).collect::<Vec<_>>()});
    depends(&pool, ready_a, refs(vec![first, first, second])).await;
    depends(&pool, ready_b, refs(vec![second])).await;
    depends(&pool, excluded, refs(vec![first])).await;
    depends(&pool, blocked_incomplete, refs(vec![first, incomplete])).await;
    depends(&pool, blocked_deleted, refs(vec![first, deleted])).await;
    depends(&pool, blocked_non_task, refs(vec![first, non_task])).await;
    depends(
        &pool,
        blocked_cross_project,
        refs(vec![first, cross_project]),
    )
    .await;
    sqlx::query("ALTER TABLE entity_properties DROP CONSTRAINT check_values_structure")
        .execute(&pool)
        .await?;
    depends(&pool, malformed, serde_json::json!({"type":"String","value":[EntityReference::new(first.to_string(), EntityType::Task)]})).await;
    depends(&pool, message_ref, serde_json::json!({"type":"EntityReference","value":[EntityReference::with_message_id(first.to_string(), EntityType::Task, Uuid::new_v4())]})).await;
    let mut transaction = pool.begin().await?;
    let mut expected = vec![ready_a, ready_b];
    expected.sort_unstable();
    assert_eq!(
        final_ready_dependents(
            &mut transaction,
            &[first, second],
            Some("task-dependencies-project-a"),
            &[excluded]
        )
        .await?,
        expected
    );
    transaction.commit().await?;
    Ok(())
}
