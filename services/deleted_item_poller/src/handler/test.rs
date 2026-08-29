use std::sync::{Arc, Mutex};

use macro_event_broker::{EventBrokerError, MacroEvent, MacroEventBroker};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use super::*;

#[derive(Clone, Debug, PartialEq)]
struct PublishedEvent {
    topic: String,
    key: String,
    envelope: Value,
}

#[derive(Clone, Default)]
enum DeliveryBehavior {
    #[default]
    Succeed,
    Fail,
    FailToJoin,
    Wait(Arc<Semaphore>),
}

#[derive(Clone, Default)]
struct FakeEventBroker {
    published: Arc<Mutex<Vec<PublishedEvent>>>,
    fail_send: bool,
    delivery_behavior: DeliveryBehavior,
}

impl FakeEventBroker {
    fn failing_send() -> Self {
        Self {
            fail_send: true,
            ..Self::default()
        }
    }

    fn failing_delivery() -> Self {
        Self {
            delivery_behavior: DeliveryBehavior::Fail,
            ..Self::default()
        }
    }

    fn failing_join() -> Self {
        Self {
            delivery_behavior: DeliveryBehavior::FailToJoin,
            ..Self::default()
        }
    }

    fn waiting_for_delivery(delivery_gate: Arc<Semaphore>) -> Self {
        Self {
            delivery_behavior: DeliveryBehavior::Wait(delivery_gate),
            ..Self::default()
        }
    }

    fn published(&self) -> Vec<PublishedEvent> {
        self.published.lock().unwrap().clone()
    }
}

impl MacroEventBroker for FakeEventBroker {
    fn send_event<E: MacroEvent + ?Sized>(
        &self,
        event: &E,
    ) -> Result<tokio::task::JoinHandle<Result<(), EventBrokerError>>, EventBrokerError> {
        if self.fail_send {
            return Err(EventBrokerError::Publish(
                "event enqueue rejected".to_string(),
            ));
        }

        self.published.lock().unwrap().push(PublishedEvent {
            topic: event.topic().to_string(),
            key: event.key().to_string(),
            envelope: serde_json::to_value(event.event())?,
        });

        let delivery_handle = match self.delivery_behavior.clone() {
            DeliveryBehavior::Succeed => tokio::spawn(async { Ok(()) }),
            DeliveryBehavior::Fail => tokio::spawn(async {
                Err(EventBrokerError::Publish(
                    "publisher unavailable".to_string(),
                ))
            }),
            DeliveryBehavior::FailToJoin => {
                let handle = tokio::spawn(std::future::pending::<Result<(), EventBrokerError>>());
                handle.abort();
                handle
            }
            DeliveryBehavior::Wait(delivery_gate) => tokio::spawn(async move {
                let permit = delivery_gate
                    .acquire_owned()
                    .await
                    .expect("delivery gate should remain open");
                permit.forget();
                Ok(())
            }),
        };

        Ok(delivery_handle)
    }
}

#[test]
fn document_purge_candidates_round_trip_exact_rfc3339_tokens_without_events() {
    let deleted_at = chrono::DateTime::parse_from_rfc3339("2026-07-01T02:03:04.123456+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let entries = document_purge_queue_entries(vec![
        macro_db_client::document::get_all_documents::DocumentPurgeCandidate {
            document_id: "document-old".into(),
            deleted_at,
        },
    ]);
    assert_eq!(
        entries,
        vec![(
            "document-old".into(),
            "2026-07-01T02:03:04.123456+00:00".into()
        )]
    );
}

#[tokio::test]
async fn project_candidates_preserve_token_order_for_purge_closure() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let expected = chrono::DateTime::parse_from_rfc3339("2026-07-01T02:03:04.123456+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let seen_by_purge = seen.clone();
    process_project_candidates(
        vec![
            macro_db_client::projects::ProjectToDelete {
                project_id: "first".into(),
                deleted_at: expected,
            },
            macro_db_client::projects::ProjectToDelete {
                project_id: "second".into(),
                deleted_at: expected + chrono::Duration::seconds(1),
            },
        ],
        move |candidate| {
            let seen = seen_by_purge.clone();
            async move {
                seen.lock()
                    .unwrap()
                    .push((candidate.project_id, candidate.deleted_at));
                Ok(ProjectPurgeOutcome::StaleOrUnavailable)
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            ("first".to_string(), expected),
            (
                "second".to_string(),
                expected + chrono::Duration::seconds(1)
            ),
        ]
    );
}

#[tokio::test]
async fn project_candidate_error_propagates_without_following_purge() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_by_purge = calls.clone();
    let error = process_project_candidates(
        vec![
            macro_db_client::projects::ProjectToDelete {
                project_id: "broken".into(),
                deleted_at: chrono::Utc::now(),
            },
            macro_db_client::projects::ProjectToDelete {
                project_id: "not-reached".into(),
                deleted_at: chrono::Utc::now(),
            },
        ],
        move |candidate| {
            let calls = calls_by_purge.clone();
            async move {
                calls.lock().unwrap().push(candidate.project_id);
                Err(projects::domain::models::ProjectError::Internal(
                    anyhow::anyhow!("purge failed"),
                ))
            }
        },
    )
    .await
    .expect_err("purge errors are returned");
    assert!(format!("{error:#}").contains("unable to purge project candidate"));
    assert_eq!(*calls.lock().unwrap(), vec!["broken"]);
}

#[tokio::test]
async fn publish_chat_purge_events_publishes_separately_keyed_events() {
    let event_broker = FakeEventBroker::default();
    let chat_ids = vec!["chat-one".to_string(), "chat-two".to_string()];

    publish_chat_purge_events(&event_broker, &chat_ids)
        .await
        .unwrap();

    let published = event_broker.published();
    assert_eq!(published.len(), chat_ids.len());

    for (event, chat_id) in published.iter().zip(&chat_ids) {
        assert_eq!(event.topic, "macro.chats");
        assert_eq!(event.key, *chat_id);
        assert_eq!(event.envelope["schema_version"], json!(1));
        assert_eq!(
            event.envelope["event_type"],
            json!("chat.permanently_deleted")
        );

        let metadata = &event.envelope["metadata"];
        assert_eq!(metadata["chat_id"], json!(chat_id));
        assert_eq!(metadata["actor_user_id"], Value::Null);
        assert_eq!(metadata["project_id"], Value::Null);
    }
}

#[tokio::test]
async fn publish_chat_purge_events_returns_immediate_send_failures() {
    let event_broker = FakeEventBroker::failing_send();
    let chat_ids = vec!["chat-one".to_string()];

    let error = publish_chat_purge_events(&event_broker, &chat_ids)
        .await
        .expect_err("immediate send failure should be returned");

    assert!(format!("{error:#}").contains("event enqueue rejected"));
}

#[tokio::test]
async fn publish_chat_purge_events_returns_delivery_failures() {
    let event_broker = FakeEventBroker::failing_delivery();
    let chat_ids = vec!["chat-one".to_string()];

    let error = publish_chat_purge_events(&event_broker, &chat_ids)
        .await
        .expect_err("delivery failure should be returned");

    assert!(format!("{error:#}").contains("publisher unavailable"));
}

#[tokio::test]
async fn publish_chat_purge_events_returns_delivery_join_failures() {
    let event_broker = FakeEventBroker::failing_join();
    let chat_ids = vec!["chat-one".to_string()];

    let error = publish_chat_purge_events(&event_broker, &chat_ids)
        .await
        .expect_err("delivery join failure should be returned");

    assert!(
        format!("{error:#}").contains("chat purge event publication task failed"),
        "unexpected error: {error:#}"
    );
}

#[tokio::test]
async fn publish_chat_purge_events_waits_for_every_delivery() {
    let delivery_gate = Arc::new(Semaphore::new(0));
    let event_broker = FakeEventBroker::waiting_for_delivery(delivery_gate.clone());
    let chat_ids = vec!["chat-one".to_string(), "chat-two".to_string()];

    let publication_task = tokio::spawn({
        let event_broker = event_broker.clone();
        let chat_ids = chat_ids.clone();
        async move { publish_chat_purge_events(&event_broker, &chat_ids).await }
    });

    while event_broker.published().len() < chat_ids.len() {
        tokio::task::yield_now().await;
    }

    assert!(!publication_task.is_finished());

    delivery_gate.add_permits(chat_ids.len());
    publication_task.await.unwrap().unwrap();
}
