use super::*;
use entity_access::domain::{
    models::EntityType as AccessEntityType, service::EntityAccessServiceImpl,
};
use entity_access::outbound::PgAccessRepository;
use macro_event_broker::{
    Event, EventBrokerError, MacroEvent, MacroEventBroker, MacroEventCollection,
};
use macro_user_id::cowlike::CowLike;
use macro_user_id::user_id::MacroUserIdStr;
use models_properties::EntityType;
use models_properties::api::requests::SetPropertyValue;
use notification::domain::models::UserNotificationRow;
use notification::domain::models::email_notification_digest::{
    BulkDigestStateMachine, StateMachineDecisionA,
};
use notification::domain::models::queue_message::{QueueMessage, RawQueueMessage};
use notification::domain::ports::NotificationQueue;
use notification::domain::service::NotificationIngressService;
use notification::outbound::repository::DbNotificationRepository;
use properties::domain::events::{
    PropertyTopicEvent, TaskReadyMetadata as SourceTaskReadyMetadata,
};
use properties::domain::model::TaskAssignedNotification;
use properties::domain::service::PropertiesService;
use properties::{
    EditReceipt, NotificationService, PermissionService, PropertiesPgRepo, PropertiesServiceImpl,
    ViewReceipt, canonical_entity_type,
};
use rootcause::Report;
use serde::Serialize;
use sqlx::{PgPool, Postgres};
use std::sync::{Arc, Mutex};
use system_properties::{StatusOption, SystemPropertyKey};
use uuid::Uuid;

static MACRO_DB_MIGRATIONS: sqlx::migrate::Migrator =
    sqlx::migrate!("../../crates/macro_db_client/migrations");

struct FakeAccess {
    denied_user: Option<&'static str>,
    fail: bool,
}
impl TaskReadyAccess for FakeAccess {
    async fn may_view(
        &self,
        _user: &macro_user_id::user_id::MacroUserIdStr<'static>,
        _task_id: Uuid,
    ) -> anyhow::Result<bool> {
        if self.fail {
            anyhow::bail!("redacted")
        } else {
            Ok(self.denied_user != Some(_user.as_ref()))
        }
    }
}
#[derive(Default)]
struct FakeIngress(Mutex<Vec<serde_json::Value>>);
impl TaskReadyIngress for FakeIngress {
    async fn enqueue(&self, request: TaskReadyRequest) -> anyhow::Result<()> {
        self.0.lock().unwrap().push(serde_json::to_value(request)?);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct RecordingPropertyBroker(Arc<Mutex<Vec<(String, Vec<u8>)>>>);

impl RecordingPropertyBroker {
    fn ready_events(&self) -> Vec<PropertyMacroEvent> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(key, payload)| PropertyMacroEvent::decode(key.clone(), payload).ok())
            .filter(|event| matches!(event.event().event, PropertyTopicEvent::TaskReady(_)))
            .collect()
    }
}

impl MacroEventBroker for RecordingPropertyBroker {
    fn send_event<E: MacroEvent + ?Sized>(
        &self,
        event: &E,
    ) -> Result<tokio::task::JoinHandle<Result<(), EventBrokerError>>, EventBrokerError> {
        self.0
            .lock()
            .unwrap()
            .push((event.key().to_string(), serde_json::to_vec(event.event())?));
        Ok(tokio::spawn(async { Ok(()) }))
    }
}

struct UnusedPermissionService;

impl PermissionService for UnusedPermissionService {
    type Err = anyhow::Error;

    async fn mint_view_receipt<'a>(
        &self,
        _user_id: Option<&'a MacroUserIdStr<'a>>,
        _entity_id: &str,
        _entity_type: AccessEntityType,
    ) -> Result<ViewReceipt, Self::Err> {
        anyhow::bail!("unused permission adapter")
    }

    async fn mint_edit_receipt<'a>(
        &self,
        _user_id: &MacroUserIdStr<'a>,
        _entity_id: &str,
        _entity_type: AccessEntityType,
    ) -> Result<EditReceipt, Self::Err> {
        anyhow::bail!("unused permission adapter")
    }

    async fn grant_permissions_to_task<'a>(
        &self,
        _user_ids: &[MacroUserIdStr<'a>],
        _task_id: &str,
    ) -> Result<(), Self::Err> {
        anyhow::bail!("unused permission adapter")
    }
}

struct UnusedNotificationService;

impl NotificationService for UnusedNotificationService {
    type Err = anyhow::Error;

    async fn send_task_assigned<'a>(
        &self,
        _notification: TaskAssignedNotification<'a>,
    ) -> Result<(), Self::Err> {
        anyhow::bail!("unused notification adapter")
    }
}

#[derive(Clone, Default)]
struct RecordingNotificationQueue(Arc<Mutex<Vec<serde_json::Value>>>);

impl RecordingNotificationQueue {
    fn published(&self) -> Vec<serde_json::Value> {
        self.0.lock().unwrap().clone()
    }
}

impl NotificationQueue for RecordingNotificationQueue {
    async fn publish<'a, T: Serialize + Send + Sync, U: Serialize + Send + Sync>(
        &self,
        messages: Vec<QueueMessage<'a, T, U>>,
    ) -> Result<(), Report> {
        self.0.lock().unwrap().extend(
            messages
                .into_iter()
                .map(serde_json::to_value)
                .collect::<Result<Vec<_>, _>>()?,
        );
        Ok(())
    }

    async fn receive_messages(&self) -> Result<Vec<RawQueueMessage>, Report> {
        Ok(Vec::new())
    }

    async fn delete_message(&self, _receipt_handle: &str) -> Result<(), Report> {
        Ok(())
    }
}

struct NoopDigestStateMachine;

impl BulkDigestStateMachine for NoopDigestStateMachine {
    async fn ingest<T: Serialize + Send + Sync + 'static>(
        &self,
        _notif: UserNotificationRow<Arc<T>>,
    ) -> Result<StateMachineDecisionA, Report> {
        Err(rootcause::report!("digest delivery is outside this test"))
    }
}
fn snapshot(
    users: &[&str],
) -> properties::outbound::task_ready_notification_queries::TaskReadyNotificationSnapshot {
    properties::outbound::task_ready_notification_queries::TaskReadyNotificationSnapshot {
        task_name: "Current task".to_string(),
        recipient_ids: users
            .iter()
            .map(|u| {
                macro_user_id::user_id::MacroUserIdStr::parse_from_str(u)
                    .unwrap()
                    .into_owned()
            })
            .collect(),
    }
}

async fn live_task(pool: &PgPool, id: Uuid, name: &str) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO "Document" (id, name, owner, "projectId")
           VALUES ($1, $2, 'task-dependencies-owner', 'task-dependencies-project-a')"#,
    )
    .bind(id.to_string())
    .bind(name)
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

async fn task_property(
    pool: &PgPool,
    task_id: Uuid,
    property_definition_id: Uuid,
    values: serde_json::Value,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4) ON CONFLICT (entity_id, entity_type, property_definition_id) DO UPDATE SET values = EXCLUDED.values",
    )
    .bind(Uuid::new_v4())
    .bind(task_id.to_string())
    .bind(property_definition_id)
    .bind(values)
    .execute(pool)
    .await?;
    Ok(())
}

async fn task_status(pool: &PgPool, task_id: Uuid, status: StatusOption) -> anyhow::Result<()> {
    task_property(
        pool,
        task_id,
        SystemPropertyKey::STATUS_UUID,
        serde_json::json!({"type":"SelectOption","value":[status.uuid()]}),
    )
    .await
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(
        path = "../../../../../crates/properties/fixtures",
        scripts("task_dependencies_seed")
    )
)]
async fn migrated_ready_snapshot_filters_access_and_suppresses_stale_event(
    pool: sqlx::Pool<Postgres>,
) -> anyhow::Result<()> {
    let predecessor_id = Uuid::from_u128(0x401);
    let task_id = Uuid::from_u128(0x402);
    let original_event_id = Uuid::from_u128(0x403);
    let stale_event_id = Uuid::from_u128(0x404);
    let permitted_user = "macro|permitted@example.com";
    let denied_user = "macro|denied@example.com";

    live_task(&pool, predecessor_id, "Completed predecessor").await?;
    live_task(&pool, task_id, "Canonical live dependent task").await?;
    task_status(&pool, predecessor_id, StatusOption::Completed).await?;
    task_status(&pool, task_id, StatusOption::NotStarted).await?;
    task_property(
        &pool,
        task_id,
        SystemPropertyKey::DEPENDS_ON_UUID,
        serde_json::json!({"type":"EntityReference","value":[{
            "entity_id": predecessor_id,
            "entity_type":"TASK",
            "specific_message_id": null
        }]}),
    )
    .await?;
    task_property(
        &pool,
        task_id,
        SystemPropertyKey::ASSIGNEES_UUID,
        serde_json::json!({"type":"EntityReference","value":[
            {"entity_id": permitted_user, "entity_type":"USER", "specific_message_id": null},
            {"entity_id": permitted_user, "entity_type":"USER", "specific_message_id": null},
            {"entity_id": denied_user, "entity_type":"USER", "specific_message_id": null}
        ]}),
    )
    .await?;

    let snapshot = properties::outbound::task_ready_notification_queries::load_current_task_ready_notification(
        &pool, task_id,
    )
    .await?
    .expect("the completed predecessor makes the live dependent task ready");
    assert_eq!(snapshot.task_name, "Canonical live dependent task");
    assert_eq!(snapshot.recipient_ids.len(), 2);

    let ingress = FakeIngress::default();
    assert_eq!(
        dispatch_snapshot(
            &FakeAccess {
                denied_user: Some(denied_user),
                fail: false,
            },
            &ingress,
            original_event_id,
            task_id,
            snapshot,
        )
        .await?,
        MaterializeOutcome::Enqueued
    );
    let requests = ingress.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request["uuid_to_write"], original_event_id.to_string());
    assert_eq!(
        request["req"]["notification_entity"]["entity_type"],
        "document"
    );
    assert_eq!(
        request["req"]["notification_entity"]["entity_id"],
        task_id.to_string()
    );
    assert_eq!(
        request["req"]["notification"]["content"]["taskId"],
        task_id.to_string()
    );
    assert_eq!(
        request["req"]["notification"]["content"]["taskName"],
        "Canonical live dependent task"
    );
    assert_eq!(
        request["req"]["recipient_ids"],
        serde_json::json!([permitted_user])
    );
    assert!(request["build_apns"].is_object());
    assert_eq!(request["send_conn_gateway"], true);
    assert!(!request.to_string().contains(denied_user));
    assert!(!request.to_string().contains(&predecessor_id.to_string()));
    drop(requests);

    task_status(&pool, predecessor_id, StatusOption::NotStarted).await?;
    let stale_snapshot = properties::outbound::task_ready_notification_queries::load_current_task_ready_notification(
        &pool, task_id,
    )
    .await?;
    assert!(
        stale_snapshot.is_none(),
        "stale event {stale_event_id} must be suppressed"
    );
    assert_eq!(ingress.0.lock().unwrap().len(), 1);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(
        path = "../../../../../crates/properties/fixtures",
        scripts("task_dependencies_seed")
    )
)]
async fn final_predecessor_completion_persists_one_permitted_notification(
    pool: sqlx::Pool<Postgres>,
) -> anyhow::Result<()> {
    let predecessor_id = Uuid::from_u128(0x411);
    let task_id = Uuid::from_u128(0x412);
    let permitted_user = "macro|permitted@example.com";
    let denied_user = "macro|denied@example.com";
    let type_disabled_user = "macro|type-disabled@example.com";

    live_task(&pool, predecessor_id, "Final predecessor").await?;
    live_task(&pool, task_id, "Ready after final predecessor").await?;
    task_status(&pool, predecessor_id, StatusOption::NotStarted).await?;
    task_status(&pool, task_id, StatusOption::NotStarted).await?;
    task_property(
        &pool,
        task_id,
        SystemPropertyKey::DEPENDS_ON_UUID,
        serde_json::json!({"type":"EntityReference","value":[{
            "entity_id": predecessor_id,
            "entity_type":"TASK",
            "specific_message_id": null
        }]}),
    )
    .await?;
    task_property(
        &pool,
        task_id,
        SystemPropertyKey::ASSIGNEES_UUID,
        serde_json::json!({"type":"EntityReference","value":[
            {"entity_id": permitted_user, "entity_type":"USER", "specific_message_id": null},
            {"entity_id": denied_user, "entity_type":"USER", "specific_message_id": null},
            {"entity_id": type_disabled_user, "entity_type":"USER", "specific_message_id": null}
        ]}),
    )
    .await?;
    sqlx::query(
        "INSERT INTO user_notification_type_preference (user_id, notification_event_type) VALUES ($1, 'task_ready')",
    )
    .bind(type_disabled_user)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO entity_access (entity_id, entity_type, source_id, source_type, access_level) VALUES ($1, 'document', $2, 'user', 'view'), ($1, 'document', $3, 'user', 'view')",
    )
    .bind(task_id)
    .bind(permitted_user)
    .bind(type_disabled_user)
    .execute(&pool)
    .await?;

    let property_broker = RecordingPropertyBroker::default();
    let properties = PropertiesServiceImpl::new(
        PropertiesPgRepo::new(pool.clone()),
        None::<UnusedPermissionService>,
        None::<UnusedNotificationService>,
    )
    .with_event_broker(property_broker.clone());
    let predecessor_receipt = EditReceipt::dangerously_assert_internal_user(
        &predecessor_id.to_string(),
        canonical_entity_type(EntityType::Task),
    );
    let completed = Some(SetPropertyValue::SelectOption {
        option_id: StatusOption::COMPLETED_UUID,
    });
    properties
        .set_entity_property(
            &predecessor_receipt,
            SystemPropertyKey::STATUS_UUID,
            completed.clone(),
        )
        .await?;
    properties
        .set_entity_property(
            &predecessor_receipt,
            SystemPropertyKey::STATUS_UUID,
            completed,
        )
        .await?;

    let ready_events = property_broker.ready_events();
    assert_eq!(
        ready_events.len(),
        1,
        "the completed retry must not publish another readiness signal"
    );
    let ready_event = &ready_events[0];
    assert_eq!(ready_event.key(), task_id.to_string());
    let event_id = ready_event.event().event_id;

    let queue = RecordingNotificationQueue::default();
    let ingress = NotificationIngressService::new(
        DbNotificationRepository::new(pool.clone()),
        queue.clone(),
        NoopDigestStateMachine,
    );
    let materializer = TaskReadyNotificationMaterializer::new(
        pool.clone(),
        EntityAccessServiceImpl::new(PgAccessRepository::new(pool.clone())),
        ingress,
    );

    for _ in 0..2 {
        assert_eq!(
            materializer.materialize(ready_event).await?,
            MaterializeOutcome::Enqueued
        );
    }

    let notification_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notification WHERE id = $1")
            .bind(event_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(notification_count, 1);
    let recipients: Vec<String> = sqlx::query_scalar(
        "SELECT user_id FROM user_notification WHERE notification_id = $1 ORDER BY user_id",
    )
    .bind(event_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(recipients, vec![permitted_user.to_string()]);
    assert_eq!(
        queue.published().len(),
        1,
        "idempotent replay must not enqueue a duplicate delivery"
    );
    Ok(())
}

#[tokio::test]
async fn dispatch_mixed_allow_and_unauthorized_sends_one_exact_request() {
    let ingress = FakeIngress::default();
    let event_id = Uuid::from_u128(1);
    let task_id = Uuid::from_u128(2);
    assert_eq!(
        dispatch_snapshot(
            &FakeAccess {
                denied_user: Some("macro|b@example.com"),
                fail: false
            },
            &ingress,
            event_id,
            task_id,
            snapshot(&["macro|a@example.com", "macro|b@example.com"])
        )
        .await
        .unwrap(),
        MaterializeOutcome::Enqueued
    );
    let sent = ingress.0.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["uuid_to_write"], event_id.to_string());
    assert_eq!(sent[0]["req"]["sender_id"], serde_json::Value::Null);
    assert_eq!(
        sent[0]["req"]["notification_entity"]["entity_id"],
        task_id.to_string()
    );
    assert_eq!(
        sent[0]["req"]["secondary_notification_entity"],
        serde_json::Value::Null
    );
    assert_eq!(
        sent[0]["req"]["notification"]["content"]["taskId"],
        task_id.to_string()
    );
    assert_eq!(sent[0]["req"]["recipient_ids"].as_array().unwrap().len(), 1);
    assert_eq!(sent[0]["req"]["recipient_ids"][0], "macro|a@example.com");
    assert_eq!(
        sent[0]["req"]["notification"]["content"]["taskName"],
        "Current task"
    );
    assert!(sent[0]["build_apns"].is_object());
    assert_eq!(sent[0]["send_conn_gateway"], true);
}

#[tokio::test]
async fn dispatch_all_denied_or_backend_failure_does_not_enqueue() {
    let ingress = FakeIngress::default();
    let id = Uuid::from_u128(2);
    assert_eq!(
        dispatch_snapshot(
            &FakeAccess {
                denied_user: Some("macro|a@example.com"),
                fail: false
            },
            &ingress,
            Uuid::nil(),
            id,
            snapshot(&["macro|a@example.com"])
        )
        .await
        .unwrap(),
        MaterializeOutcome::Ignored
    );
    assert!(
        dispatch_snapshot(
            &FakeAccess {
                denied_user: None,
                fail: true
            },
            &ingress,
            Uuid::nil(),
            id,
            snapshot(&["macro|a@example.com"])
        )
        .await
        .is_err()
    );
    assert!(ingress.0.lock().unwrap().is_empty());
}

#[test]
fn task_ready_consumer_group_is_durable_and_exact() {
    assert_eq!(
        TaskReadyNotificationConsumerGroup::GROUP_NAME,
        "task-ready-notification-materializer"
    );
    assert_eq!(TaskReadyDeclaredEvent::topics().len(), 1);
}

#[test]
fn malformed_payload_is_ignored_and_commit_class() {
    let poison: Result<TaskReadyDeclaredEvent, macro_event_broker::EventBrokerError> =
        Err(macro_event_broker::EventBrokerError::MissingMessagePayload);
    assert!(decoded_event_or_none(poison).is_none());
    assert_eq!(
        commit_decision(&Ok(MaterializeOutcome::Ignored)),
        CommitDecision::Commit
    );
}

#[test]
fn query_access_or_ingress_failure_is_a_no_commit_retry() {
    let failure = Err(anyhow::anyhow!("redacted processing failure"));
    assert_eq!(commit_decision(&failure), CommitDecision::Retry);
}

#[test]
fn supervisor_cancellation_has_no_restart_delay() {
    assert!(supervisor_restart_delay(true, 0).is_none());
    assert_eq!(supervisor_restart_delay(false, 0).unwrap().as_secs(), 1);
}

fn ready_event(key: String, schema_version: u8) -> PropertyMacroEvent {
    let task_id = Uuid::from_u128(9);
    PropertyMacroEvent::from_event(
        key,
        Event::with_event_id_and_schema_version(
            Uuid::from_u128(10),
            schema_version,
            PropertyTopicEvent::TaskReady(SourceTaskReadyMetadata { task_id }),
        ),
    )
}

#[test]
fn constructed_non_target_schema_and_key_mismatch_are_ignored() {
    let task_id = Uuid::from_u128(9);
    assert!(!should_ignore_event(&ready_event(task_id.to_string(), 1)));
    assert!(should_ignore_event(&ready_event("wrong".to_string(), 1)));
    assert!(should_ignore_event(&ready_event(task_id.to_string(), 2)));
    let non_target = PropertyMacroEvent::from_event(
        "other".to_string(),
        Event::with_event_id(
            Uuid::from_u128(12),
            PropertyTopicEvent::EntityPropertiesCleared(
                properties::domain::events::EntityPropertiesClearedMetadata {
                    entity_id: "other".to_string(),
                    entity_type: models_properties::EntityType::Task,
                    actor_user_id: None,
                    actor: None,
                    on_behalf_of: None,
                },
            ),
        ),
    );
    assert!(should_ignore_event(&non_target));
}
