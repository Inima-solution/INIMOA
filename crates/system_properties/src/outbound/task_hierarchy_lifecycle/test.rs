use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::reconcile_relocated_task_hierarchy;
use crate::SystemPropertyKey;

async fn schema(pool: &PgPool) -> anyhow::Result<()> {
    for statement in [
        "CREATE TABLE \"Document\" (id text PRIMARY KEY)",
        "CREATE TABLE document_sub_type (document_id text, sub_type text)",
        "CREATE TABLE entity_properties (id uuid PRIMARY KEY, entity_id text, entity_type text, property_definition_id uuid, values jsonb, updated_at timestamptz, UNIQUE(entity_id, entity_type, property_definition_id))",
    ] {
        sqlx::query(statement).execute(pool).await?;
    }
    Ok(())
}

async fn property(
    pool: &PgPool,
    entity_id: &str,
    definition: Uuid,
    value: Value,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
        .bind(Uuid::new_v4()).bind(entity_id).bind(definition).bind(value).execute(pool).await?;
    Ok(())
}

async fn values(pool: &PgPool, entity_id: &str, definition: Uuid) -> anyhow::Result<Option<Value>> {
    Ok(sqlx::query(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2",
    )
    .bind(entity_id)
    .bind(definition)
    .fetch_one(pool)
    .await?
    .try_get(0)?)
}

#[sqlx::test]
async fn relocated_task_clears_only_incident_hierarchy_edges(pool: PgPool) -> anyhow::Result<()> {
    schema(&pool).await?;
    let source = Uuid::new_v4().to_string();
    let peer = Uuid::new_v4().to_string();
    let other = Uuid::new_v4().to_string();
    for id in [&source, &peer, &other] {
        sqlx::query("INSERT INTO \"Document\" (id) VALUES ($1)")
            .bind(id)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
            .bind(id)
            .execute(&pool)
            .await?;
    }
    let parent = SystemPropertyKey::PARENT_TASK_UUID;
    let subtasks = SystemPropertyKey::SUBTASKS_UUID;
    let exact_other =
        json!({"entity_id": other, "entity_type":"TASK", "specific_message_id":"keep"});
    property(
        &pool,
        &source,
        parent,
        json!({"type":"EntityReference","value":[{"entity_id":peer,"entity_type":"TASK"}]}),
    )
    .await?;
    property(&pool, &source, subtasks, json!({"type":"EntityReference","value":[exact_other.clone(), {"entity_id":peer,"entity_type":"TASK"}]})).await?;
    property(&pool, &peer, parent, json!({"type":"EntityReference","value":[{"entity_id":source,"entity_type":"TASK"}, exact_other.clone()]})).await?;
    property(&pool, &peer, subtasks, json!({"type":"EntityReference","value":[{"entity_id":source,"entity_type":"TASK"}, exact_other.clone()]})).await?;
    property(
        &pool,
        &other,
        SystemPropertyKey::STATUS_UUID,
        json!({"type":"SelectOption","value":[]}),
    )
    .await?;
    property(
        &pool,
        &other,
        SystemPropertyKey::DEPENDS_ON_UUID,
        json!({"type":"EntityReference","value":[{"entity_id":source,"entity_type":"TASK"}]}),
    )
    .await?;
    let mut transaction = pool.begin().await?;
    reconcile_relocated_task_hierarchy(&mut transaction, &source).await?;
    transaction.commit().await?;
    assert_eq!(values(&pool, &source, parent).await?, None);
    assert_eq!(values(&pool, &source, subtasks).await?, None);
    assert_eq!(
        values(&pool, &peer, parent).await?,
        Some(json!({"type":"EntityReference","value":[exact_other.clone()]}))
    );
    assert_eq!(
        values(&pool, &peer, subtasks).await?,
        Some(json!({"type":"EntityReference","value":[exact_other]}))
    );
    assert!(
        values(&pool, &other, SystemPropertyKey::STATUS_UUID)
            .await?
            .is_some()
    );
    assert!(
        values(&pool, &other, SystemPropertyKey::DEPENDS_ON_UUID)
            .await?
            .is_some()
    );
    Ok(())
}

#[sqlx::test]
async fn zero_remaining_malformed_source_and_non_task_source_are_safe(
    pool: PgPool,
) -> anyhow::Result<()> {
    schema(&pool).await?;
    let source = Uuid::new_v4().to_string();
    let peer = Uuid::new_v4().to_string();
    for id in [&source, &peer] {
        sqlx::query("INSERT INTO \"Document\" (id) VALUES ($1)")
            .bind(id)
            .execute(&pool)
            .await?;
    }
    sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
        .bind(&source)
        .execute(&pool)
        .await?;
    property(
        &pool,
        &source,
        SystemPropertyKey::PARENT_TASK_UUID,
        json!({"malformed":true}),
    )
    .await?;
    property(
        &pool,
        &peer,
        SystemPropertyKey::SUBTASKS_UUID,
        json!({"type":"EntityReference","value":[{"entity_id":source,"entity_type":"TASK"}]}),
    )
    .await?;
    let mut transaction = pool.begin().await?;
    reconcile_relocated_task_hierarchy(&mut transaction, &source).await?;
    transaction.commit().await?;
    assert_eq!(
        values(&pool, &source, SystemPropertyKey::PARENT_TASK_UUID).await?,
        None
    );
    assert_eq!(
        values(&pool, &peer, SystemPropertyKey::SUBTASKS_UUID).await?,
        None
    );
    let non_task = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO \"Document\" (id) VALUES ($1)")
        .bind(&non_task)
        .execute(&pool)
        .await?;
    let original =
        json!({"type":"EntityReference","value":[{"entity_id":non_task,"entity_type":"TASK"}]});
    property(
        &pool,
        &peer,
        SystemPropertyKey::PARENT_TASK_UUID,
        original.clone(),
    )
    .await?;
    let mut transaction = pool.begin().await?;
    reconcile_relocated_task_hierarchy(&mut transaction, &non_task).await?;
    transaction.commit().await?;
    assert_eq!(
        values(&pool, &peer, SystemPropertyKey::PARENT_TASK_UUID).await?,
        Some(original)
    );
    Ok(())
}
