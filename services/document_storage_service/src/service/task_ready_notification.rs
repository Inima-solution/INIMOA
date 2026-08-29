//! Kafka materializer for authoritative task-ready notifications.

use std::{collections::HashSet, future::Future};

use entity_access::domain::{
    models::{AccessError, EntityType, ViewAccessLevel},
    ports::EntityAccessService,
};
use kafka_util::{GroupName, KafkaEventConsumer};
use macro_event_broker::{KafkaConsumerAdapter, MacroEvent as _, MacroEventConsumerService};
use model_entity::EntityType as NotificationEntityType;
use model_notifications::TaskReadyMetadata;
use notification::domain::{
    models::{
        SendNotificationRequest, SendNotificationRequestBuilder, apple::PushNotificationData,
    },
    service::NotificationIngress,
};
use properties::{
    domain::events::{PropertyMacroEvent, PropertyTopicEvent},
    outbound::task_ready_notification_queries::load_current_task_ready_notification,
};
use rdkafka::consumer::CommitMode;
use sqlx::PgPool;
use uuid::Uuid;

type TaskReadyRequest = SendNotificationRequest<'static, TaskReadyMetadata, PushNotificationData>;

trait TaskReadyAccess {
    fn may_view(
        &self,
        user: &macro_user_id::user_id::MacroUserIdStr<'static>,
        task_id: Uuid,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
}
impl<A: EntityAccessService> TaskReadyAccess for A {
    fn may_view(
        &self,
        user: &macro_user_id::user_id::MacroUserIdStr<'static>,
        task_id: Uuid,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send {
        async move {
            match self
                .generate_entity_access_receipt::<ViewAccessLevel>(
                    std::ops::Deref::deref(user),
                    None,
                    &task_id.to_string(),
                    EntityType::Document,
                )
                .await
            {
                Ok(_) => Ok(true),
                Err(AccessError::Unauthorized | AccessError::UnauthorizedWithMessage(_)) => {
                    Ok(false)
                }
                Err(_) => Err(anyhow::anyhow!("task-ready access check failed")),
            }
        }
    }
}
trait TaskReadyIngress {
    fn enqueue(&self, request: TaskReadyRequest)
    -> impl Future<Output = anyhow::Result<()>> + Send;
}
impl<N: NotificationIngress> TaskReadyIngress for N {
    fn enqueue(
        &self,
        request: TaskReadyRequest,
    ) -> impl Future<Output = anyhow::Result<()>> + Send {
        async move {
            self.send_notification(request)
                .await
                .map(|_| ())
                .map_err(|_| anyhow::anyhow!("task-ready notification ingress failed"))
        }
    }
}

async fn dispatch_snapshot<A: TaskReadyAccess, N: TaskReadyIngress>(
    access: &A,
    ingress: &N,
    event_id: Uuid,
    task_id: Uuid,
    snapshot: properties::outbound::task_ready_notification_queries::TaskReadyNotificationSnapshot,
) -> anyhow::Result<MaterializeOutcome> {
    let mut recipient_ids = HashSet::new();
    for assignee in snapshot.recipient_ids {
        if access.may_view(&assignee, task_id).await? {
            recipient_ids.insert(assignee);
        }
    }
    if recipient_ids.is_empty() {
        return Ok(MaterializeOutcome::Ignored);
    }
    let request = SendNotificationRequestBuilder {
        notification_entity: NotificationEntityType::Document
            .with_entity_string(task_id.to_string()),
        secondary_notification_entity: None,
        notification: TaskReadyMetadata {
            task_id: task_id.to_string(),
            task_name: snapshot.task_name,
        },
        sender_id: None,
        recipient_ids,
    }
    .into_request_with_id(event_id)
    .with_apns()
    .with_conn_gateway();
    ingress.enqueue(request).await?;
    Ok(MaterializeOutcome::Enqueued)
}

/// Durable consumer-group identity for task-ready notification materialization.
pub struct TaskReadyNotificationConsumerGroup;

impl GroupName for TaskReadyNotificationConsumerGroup {
    const GROUP_NAME: &'static str = "task-ready-notification-materializer";
}

macro_event_broker::declare_topics!(TaskReadyDeclaredEvent: PropertyMacroEvent);

type TaskReadyKafkaAdapter =
    KafkaConsumerAdapter<TaskReadyNotificationConsumerGroup, TaskReadyDeclaredEvent>;
type TaskReadyKafkaConsumer =
    MacroEventConsumerService<TaskReadyDeclaredEvent, TaskReadyKafkaAdapter>;

/// A completed event must be committed. An error is returned to the supervisor
/// without committing so the broker replays the event after restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializeOutcome {
    Ignored,
    Enqueued,
}

pub(crate) fn should_ignore_event(event: &PropertyMacroEvent) -> bool {
    let envelope = event.event();
    !matches!(&envelope.event, PropertyTopicEvent::TaskReady(_))
        || envelope.schema_version != 1
        || matches!(&envelope.event, PropertyTopicEvent::TaskReady(metadata) if event.key() != metadata.task_id.to_string())
}

pub(crate) fn supervisor_restart_delay(
    cancelled: bool,
    consecutive_failures: u32,
) -> Option<std::time::Duration> {
    (!cancelled).then(|| {
        std::time::Duration::from_secs(1_u64 << consecutive_failures.saturating_sub(1).min(6))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitDecision {
    Commit,
    Retry,
}

fn commit_decision(result: &anyhow::Result<MaterializeOutcome>) -> CommitDecision {
    match result {
        Ok(_) => CommitDecision::Commit,
        Err(_) => CommitDecision::Retry,
    }
}

fn decoded_event_or_none(
    decoded: Result<TaskReadyDeclaredEvent, macro_event_broker::EventBrokerError>,
) -> Option<PropertyMacroEvent> {
    match decoded {
        Ok(TaskReadyDeclaredEvent::PropertyMacroEvent(event)) => Some(event),
        Err(_) => None,
    }
}

/// Re-reads authoritative task state, filters current assignees by document
/// permission, then sends one idempotent ingress request for all recipients.
pub struct TaskReadyNotificationMaterializer<A, N> {
    pool: PgPool,
    entity_access: A,
    notification_ingress: N,
}

impl<A, N> TaskReadyNotificationMaterializer<A, N>
where
    A: EntityAccessService,
    N: NotificationIngress,
{
    pub fn new(pool: PgPool, entity_access: A, notification_ingress: N) -> Self {
        Self {
            pool,
            entity_access,
            notification_ingress,
        }
    }

    pub async fn materialize(
        &self,
        event: &PropertyMacroEvent,
    ) -> anyhow::Result<MaterializeOutcome> {
        let envelope = event.event();
        if should_ignore_event(event) {
            return Ok(MaterializeOutcome::Ignored);
        }
        let PropertyTopicEvent::TaskReady(metadata) = &envelope.event else {
            unreachable!()
        };
        let Some(snapshot) =
            load_current_task_ready_notification(&self.pool, metadata.task_id).await?
        else {
            return Ok(MaterializeOutcome::Ignored);
        };
        dispatch_snapshot(
            &self.entity_access,
            &self.notification_ingress,
            envelope.event_id,
            metadata.task_id,
            snapshot,
        )
        .await
    }
}

/// Runs one consumer instance until shutdown. Poison payloads, non-target
/// events and invalid schema/key pairs commit as intentionally ignored. Any
/// query/access/enqueue failure returns without committing its record.
pub async fn run_task_ready_notification_consumer<A, N>(
    brokers: &str,
    materializer: TaskReadyNotificationMaterializer<A, N>,
    shutdown: impl Future<Output = ()> + Send,
) -> anyhow::Result<()>
where
    A: EntityAccessService,
    N: NotificationIngress,
{
    let consumer = KafkaEventConsumer::<TaskReadyNotificationConsumerGroup>::from_env(brokers)?;
    let consumer = KafkaConsumerAdapter::<TaskReadyNotificationConsumerGroup, ()>::new(consumer)
        .subscribe::<TaskReadyDeclaredEvent>()?;
    let consumer = TaskReadyKafkaConsumer::new(consumer);
    let mut shutdown = std::pin::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            result = consumer.recv() => {
                let message = result?;
                let kafka_message = message.inner();
                let result = match decoded_event_or_none(message.decode_payload()) {
                    Some(event) => materializer.materialize(&event).await,
                    None => Ok(MaterializeOutcome::Ignored),
                };
                if commit_decision(&result) == CommitDecision::Retry {
                    return result.map(|_| ());
                }
                consumer.inner().commit_message(kafka_message, CommitMode::Sync)
                    .map_err(|error| anyhow::anyhow!("failed to commit task-ready event offset: {error:?}"))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod test;
