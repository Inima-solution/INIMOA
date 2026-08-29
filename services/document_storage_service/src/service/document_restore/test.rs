use std::sync::Mutex;

use macro_event_broker::{EventBrokerError, MacroEvent, MacroEventBroker};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::{restore_document, schedule_task_ready_events};

#[derive(Default)]
struct RecordingBroker(Mutex<Vec<(String, String, serde_json::Value)>>);

impl MacroEventBroker for RecordingBroker {
    fn send_event<E: MacroEvent + ?Sized>(
        &self,
        event: &E,
    ) -> Result<tokio::task::JoinHandle<Result<(), EventBrokerError>>, EventBrokerError> {
        self.0.lock().unwrap().push((
            event.topic().to_owned(),
            event.key().to_owned(),
            serde_json::to_value(event.event()).unwrap(),
        ));
        Ok(tokio::spawn(async { Ok(()) }))
    }
}

struct FailingBroker;

impl MacroEventBroker for FailingBroker {
    fn send_event<E: MacroEvent + ?Sized>(
        &self,
        _: &E,
    ) -> Result<tokio::task::JoinHandle<Result<(), EventBrokerError>>, EventBrokerError> {
        Err(EventBrokerError::Publish("rejected".to_owned()))
    }
}

#[tokio::test]
async fn schedules_exactly_one_task_ready_event_per_committed_task_id() {
    let broker = RecordingBroker::default();
    let task_id = Uuid::now_v7();
    schedule_task_ready_events(&broker, &[task_id]);
    let events = broker.0.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "macro.properties");
    assert_eq!(events[0].1, task_id.to_string());
    assert_eq!(events[0].2["event_type"], "task.ready");
    assert_eq!(
        events[0].2["metadata"],
        serde_json::json!({ "task_id": task_id })
    );
    assert_eq!(events[0].2["metadata"].as_object().unwrap().len(), 1);
}

#[tokio::test]
async fn immediate_broker_failure_is_dropped_after_commit() {
    schedule_task_ready_events(&FailingBroker, &[Uuid::now_v7()]);
}

async fn schema(pool: &PgPool) -> anyhow::Result<()> {
    for statement in [
        "CREATE TABLE \"Document\" (id text PRIMARY KEY, owner text NOT NULL, \"projectId\" text, \"deletedAt\" timestamptz)",
        "CREATE TABLE \"Project\" (id text PRIMARY KEY, \"deletedAt\" timestamptz)",
        "CREATE TABLE \"UserHistory\" (\"userId\" text, \"itemId\" text, \"itemType\" text, \"createdAt\" timestamptz, \"updatedAt\" timestamptz, UNIQUE(\"userId\", \"itemId\", \"itemType\"))",
        "CREATE TABLE document_sub_type (document_id text, sub_type text)",
        "CREATE TABLE entity_properties (id uuid PRIMARY KEY, entity_id text, entity_type text, property_definition_id uuid, values jsonb, UNIQUE(entity_id, entity_type, property_definition_id))",
    ] {
        sqlx::query(statement).execute(pool).await?;
    }
    Ok(())
}
async fn task(pool: &PgPool, id: Uuid, deleted: bool) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO \"Document\" (id, owner, \"deletedAt\") VALUES ($1, 'owner', CASE WHEN $2 THEN NOW() ELSE NULL END)").bind(id.to_string()).bind(deleted).execute(pool).await?;
    sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}
async fn prop(
    pool: &PgPool,
    id: Uuid,
    definition: Uuid,
    value: serde_json::Value,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)").bind(Uuid::new_v4()).bind(id.to_string()).bind(definition).bind(value).execute(pool).await?;
    Ok(())
}
async fn ready_fixture(pool: &PgPool) -> anyhow::Result<(Uuid, Uuid)> {
    let predecessor = Uuid::new_v4();
    let dependent = Uuid::new_v4();
    task(pool, predecessor, true).await?;
    task(pool, dependent, false).await?;
    prop(pool, predecessor, system_properties::SystemPropertyKey::STATUS_UUID, serde_json::json!({"type":"SelectOption","value":[system_properties::StatusOption::COMPLETED_UUID]})).await?;
    prop(pool, dependent, system_properties::SystemPropertyKey::DEPENDS_ON_UUID, serde_json::json!({"type":"EntityReference","value":[{"entity_id":predecessor,"entity_type":"TASK","specific_message_id":null}]})).await?;
    Ok((predecessor, dependent))
}

#[sqlx::test]
async fn coordinator_emits_once_for_actual_restore_and_not_retry(
    pool: PgPool,
) -> anyhow::Result<()> {
    schema(&pool).await?;
    let (predecessor, dependent) = ready_fixture(&pool).await?;
    let broker = RecordingBroker::default();
    assert_eq!(
        restore_document(&pool, &broker, &predecessor.to_string()).await?,
        vec![dependent]
    );
    assert_eq!(
        restore_document(&pool, &broker, &predecessor.to_string()).await?,
        Vec::<Uuid>::new()
    );
    let events = broker.0.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "macro.properties");
    assert_eq!(events[0].1, dependent.to_string());
    assert_eq!(events[0].2["schema_version"], 1);
    assert_eq!(events[0].2["event_type"], "task.ready");
    assert_eq!(
        events[0].2["metadata"],
        serde_json::json!({"task_id":dependent})
    );
    assert_eq!(events[0].2["metadata"].as_object().unwrap().len(), 1);
    Ok(())
}

#[sqlx::test]
async fn coordinator_blocks_incomplete_and_rolls_back_fanout_failure(
    pool: PgPool,
) -> anyhow::Result<()> {
    schema(&pool).await?;
    let (predecessor, dependent) = ready_fixture(&pool).await?;
    let incomplete = Uuid::new_v4();
    task(&pool, incomplete, false).await?;
    prop(&pool, dependent, system_properties::SystemPropertyKey::STATUS_UUID, serde_json::json!({"type":"SelectOption","value":[system_properties::StatusOption::COMPLETED_UUID]})).await?;
    sqlx::query("UPDATE entity_properties SET values = $1 WHERE entity_id = $2 AND property_definition_id = $3").bind(serde_json::json!({"type":"EntityReference","value":[{"entity_id":predecessor,"entity_type":"TASK","specific_message_id":null},{"entity_id":incomplete,"entity_type":"TASK","specific_message_id":null}]})).bind(dependent.to_string()).bind(system_properties::SystemPropertyKey::DEPENDS_ON_UUID).execute(&pool).await?;
    let broker = RecordingBroker::default();
    assert_eq!(
        restore_document(&pool, &broker, &predecessor.to_string()).await?,
        Vec::<Uuid>::new()
    );
    assert!(broker.0.lock().unwrap().is_empty());
    let failed = Uuid::new_v4();
    task(&pool, failed, true).await?;
    sqlx::query("DROP TABLE entity_properties")
        .execute(&pool)
        .await?;
    assert!(
        restore_document(&pool, &broker, &failed.to_string())
            .await
            .is_err()
    );
    let deleted: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query("SELECT \"deletedAt\"::timestamptz FROM \"Document\" WHERE id = $1")
            .bind(failed.to_string())
            .fetch_one(&pool)
            .await?
            .try_get(0)?;
    assert!(deleted.is_some());
    assert!(broker.0.lock().unwrap().is_empty());
    Ok(())
}

#[sqlx::test]
async fn immediate_broker_failure_keeps_committed_restore_successful(
    pool: PgPool,
) -> anyhow::Result<()> {
    schema(&pool).await?;
    let (predecessor, dependent) = ready_fixture(&pool).await?;
    assert_eq!(
        restore_document(&pool, &FailingBroker, &predecessor.to_string()).await?,
        vec![dependent]
    );
    let deleted: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query("SELECT \"deletedAt\"::timestamptz FROM \"Document\" WHERE id = $1")
            .bind(predecessor.to_string())
            .fetch_one(&pool)
            .await?
            .try_get(0)?;
    assert!(deleted.is_none());
    Ok(())
}
