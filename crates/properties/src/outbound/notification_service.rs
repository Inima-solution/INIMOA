//! Notification service implementation for properties.

use std::collections::HashSet;

use futures::future::join_all;
use macro_user_id::cowlike::CowLike;
use notification::domain::models::SendNotificationRequestBuilder;
use notification::domain::service::NotificationIngress;
use sqlx::{Pool, Postgres};

use super::entity_info_queries;
use crate::domain::model::TaskAssignedNotification;
use crate::domain::ports::NotificationService;

/// Notification service implementation using the new notification client.
///
/// Enriches domain-level notifications with display data (task name, sender
/// profile picture) and fans out one notification per recipient.
pub struct NotificationServiceImpl<T> {
    notification_client: T,
    pool: Pool<Postgres>,
}

impl<T> NotificationServiceImpl<T>
where
    T: NotificationIngress,
{
    /// Create a new notification service with the notification client.
    pub fn new(notification_client: T, pool: Pool<Postgres>) -> Self {
        Self {
            notification_client,
            pool,
        }
    }
}

impl<T> NotificationService for NotificationServiceImpl<T>
where
    T: NotificationIngress,
{
    type Err = anyhow::Error;

    #[tracing::instrument(skip(self, notification), fields(task_id = %notification.task_id), err)]
    async fn send_task_assigned<'a>(
        &self,
        notification: TaskAssignedNotification<'a>,
    ) -> Result<(), Self::Err> {
        // Tasks are stored as documents, so the task name is the document name.
        let task_name =
            entity_info_queries::get_document_name(&self.pool, &notification.task_id.to_string())
                .await
                .ok()
                .flatten();

        let sender_profile_picture_url = entity_info_queries::get_user_profile_picture(
            &self.pool,
            notification.assigned_by.as_ref(),
        )
        .await
        .ok()
        .flatten();

        let assigned_by = notification.assigned_by.into_owned();
        let notification_entity =
            model_entity::EntityType::Document.with_entity_string(notification.task_id.to_string());

        let notification_futures: Vec<_> = notification
            .recipient_ids
            .iter()
            .map(|recipient_id| {
                let metadata = model_notifications::TaskAssignedMetadata {
                    task_id: notification.task_id.to_string(),
                    task_name: task_name.clone(),
                    sub_type: Some(model_notifications::NotificationDocumentSubType::Task),
                    assigned_by: assigned_by.clone(),
                    sender_profile_picture_url: sender_profile_picture_url.clone(),
                };

                let request = SendNotificationRequestBuilder {
                    notification_entity: notification_entity.clone(),
                    secondary_notification_entity: None,
                    notification: metadata,
                    sender_id: Some(assigned_by.clone()),
                    recipient_ids: HashSet::from([recipient_id.copied()]),
                }
                .into_request()
                .with_apns()
                .with_conn_gateway();

                async move {
                    let send_result = self.notification_client.send_notification(request).await;
                    match send_result {
                        Ok(result) => {
                            tracing::debug!(
                                recipient_id = %recipient_id,
                                notification_id = ?result.map(|r| r.notification_id),
                                "sent task assignment notification"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                recipient_id = %recipient_id,
                                error = ?e,
                                "failed to send task assignment notification"
                            );
                        }
                    }
                }
            })
            .collect();

        join_all(notification_futures).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use macro_db_migrator::MACRO_DB_MIGRATIONS;
    use macro_user_id::user_id::MacroUserIdStr;
    use notification::domain::{
        models::{Notification, NotificationResult, SendNotificationRequest},
        service::SendNotificationError,
    };
    use rootcause::Report;
    use serde::Serialize;
    use sqlx::{Pool, Postgres};
    use uuid::Uuid;

    use super::*;

    /// Captures the serialized ingress request, which is the wire contract
    /// handed to the notification service.
    #[derive(Clone, Default)]
    struct CapturingIngress {
        sent: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    impl CapturingIngress {
        fn only_request(&self) -> serde_json::Value {
            let sent = self.sent.lock().expect("capture lock is available");
            assert_eq!(sent.len(), 1, "one recipient produces one request");
            sent[0].clone()
        }
    }

    impl NotificationIngress for CapturingIngress {
        async fn send_notification<
            'a,
            T: Notification + Clone + 'static,
            U: Serialize + Send + Sync + 'static,
        >(
            &'a self,
            request: SendNotificationRequest<'a, T, U>,
        ) -> Result<Option<NotificationResult<'a>>, Report<SendNotificationError>> {
            self.sent
                .lock()
                .expect("capture lock is available")
                .push(serde_json::to_value(request).expect("request serializes"));
            Ok(None)
        }
    }

    fn user_id(value: &str) -> MacroUserIdStr<'static> {
        MacroUserIdStr::parse_from_str(value)
            .expect("valid macro user id")
            .into_owned()
    }

    #[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
    async fn task_assignment_uses_document_notification_entity_and_task_metadata(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        let task_id = Uuid::from_u128(0x20000001_0000_0000_0000_000000000001);
        let ingress = CapturingIngress::default();
        let service = NotificationServiceImpl::new(ingress.clone(), pool);

        service
            .send_task_assigned(TaskAssignedNotification {
                task_id,
                assigned_by: user_id("macro|assigner@example.com"),
                recipient_ids: vec![user_id("macro|recipient@example.com")],
            })
            .await?;

        let request = ingress.only_request();
        let entity = &request["req"]["notification_entity"];
        assert_eq!(entity["entity_type"], "document");
        assert_eq!(entity["entity_id"], task_id.to_string());

        let metadata = &request["req"]["notification"];
        assert_eq!(metadata["tag"], "task_assigned");
        assert_eq!(metadata["content"]["taskId"], task_id.to_string());
        assert_eq!(
            metadata["content"]["subType"],
            serde_json::json!({ "type": "task" })
        );

        Ok(())
    }
}
