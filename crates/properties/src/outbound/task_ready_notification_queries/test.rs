use super::{canonical_user_id, load_current_task_ready_notification};
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use models_properties::{EntityReference, EntityType};
use sqlx::{Pool, Postgres};
use system_properties::{StatusOption, SystemPropertyKey};
use uuid::Uuid;

async fn task(pool: &Pool<Postgres>, id: Uuid, name: &str, is_task: bool) {
    sqlx::query(
        "INSERT INTO \"Document\" (id, name, owner) VALUES ($1, $2, 'task-dependencies-owner')",
    )
    .bind(id.to_string())
    .bind(name)
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

async fn prop(pool: &Pool<Postgres>, task_id: Uuid, definition: Uuid, value: serde_json::Value) {
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1,$2,'TASK',$3,$4)")
        .bind(Uuid::new_v4()).bind(task_id.to_string()).bind(definition).bind(value).execute(pool).await.unwrap();
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn current_live_ready_task_returns_exact_name_and_deduplicated_users(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let task_id = Uuid::new_v4();
    task(&pool, task_id, "Current task", true).await;
    prop(
        &pool,
        task_id,
        SystemPropertyKey::STATUS_UUID,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::NotStarted.uuid()]}),
    )
    .await;
    prop(&pool, task_id, SystemPropertyKey::ASSIGNEES_UUID, serde_json::json!({"type":"EntityReference","value":[{"entity_type":"USER","entity_id":"macro|a@example.com"},{"entity_type":"USER","entity_id":"macro|a@example.com"},{"entity_type":"TASK","entity_id":"x"}]})).await;
    let actual = load_current_task_ready_notification(&pool, task_id)
        .await?
        .unwrap();
    assert_eq!(actual.task_name, "Current task");
    assert_eq!(actual.recipient_ids.len(), 1);
    assert_eq!(actual.recipient_ids[0].as_ref(), "macro|a@example.com");
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn blocked_canceled_malformed_deleted_non_task_and_unassigned_suppress(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let blocked = Uuid::new_v4();
    let canceled = Uuid::new_v4();
    let malformed = Uuid::new_v4();
    let deleted = Uuid::new_v4();
    let non_task = Uuid::new_v4();
    let unassigned = Uuid::new_v4();
    for id in [blocked, canceled, malformed, deleted, non_task, unassigned] {
        task(&pool, id, "task", id != non_task).await;
    }
    let predecessor = Uuid::new_v4();
    task(&pool, predecessor, "predecessor", true).await;
    for id in [blocked, canceled, malformed, deleted, non_task] {
        prop(&pool, id, SystemPropertyKey::ASSIGNEES_UUID, serde_json::json!({"type":"EntityReference","value":[{"entity_type":"USER","entity_id":"macro|guard@example.com"}]})).await;
    }
    prop(
        &pool,
        canceled,
        SystemPropertyKey::STATUS_UUID,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::Canceled.uuid()]}),
    )
    .await;
    prop(
        &pool,
        malformed,
        SystemPropertyKey::DEPENDS_ON_UUID,
        serde_json::json!({"bad":true}),
    )
    .await;
    prop(&pool, blocked, SystemPropertyKey::DEPENDS_ON_UUID, serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":predecessor}]})).await;
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\"=NOW() WHERE id=$1")
        .bind(deleted.to_string())
        .execute(&pool)
        .await?;
    for id in [blocked, canceled, malformed, deleted, non_task, unassigned] {
        assert!(
            load_current_task_ready_notification(&pool, id)
                .await?
                .is_none()
        );
    }
    Ok(())
}

#[test]
fn canonical_user_references_are_deduplicable_and_message_free() {
    let reference = EntityReference {
        entity_id: "macro|assignee@example.com".to_string(),
        entity_type: EntityType::User,
        specific_message_id: None,
    };
    assert_eq!(
        canonical_user_id(reference).unwrap().as_ref(),
        "macro|assignee@example.com"
    );
}

#[test]
fn malformed_non_user_and_message_references_are_omitted() {
    for reference in [
        EntityReference {
            entity_id: "bad".to_string(),
            entity_type: EntityType::User,
            specific_message_id: None,
        },
        EntityReference {
            entity_id: "macro|u@example.com".to_string(),
            entity_type: EntityType::Task,
            specific_message_id: None,
        },
        EntityReference {
            entity_id: "macro|u@example.com".to_string(),
            entity_type: EntityType::User,
            specific_message_id: Some(Uuid::from_u128(1)),
        },
    ] {
        assert!(canonical_user_id(reference).is_none());
    }
}
