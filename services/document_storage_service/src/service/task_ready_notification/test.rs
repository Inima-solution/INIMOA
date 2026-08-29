use super::*;
use macro_event_broker::Event;
use macro_user_id::cowlike::CowLike;
use properties::domain::events::{
    PropertyTopicEvent, TaskReadyMetadata as SourceTaskReadyMetadata,
};
use std::sync::Mutex;
use uuid::Uuid;

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
