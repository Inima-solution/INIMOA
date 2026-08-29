//! Post-commit coordination for direct legacy document restoration.

use macro_event_broker::MacroEventBroker;
use properties::domain::events::{PropertyMacroEvent, TaskReadyMetadata};
use sqlx::PgPool;
use uuid::Uuid;

/// Restore one document atomically, then make exactly one best-effort task
/// readiness scheduling attempt for each final-ready dependent returned by the
/// committed transaction.  Broker failures never turn a committed restore
/// into an API failure.
pub(crate) async fn restore_document<B: MacroEventBroker>(
    db: &PgPool,
    event_broker: &B,
    document_id: &str,
) -> anyhow::Result<Vec<Uuid>> {
    let ready_task_ids =
        macro_db_client::document::revert_delete::revert_delete_document(db, document_id).await?;
    schedule_task_ready_events(event_broker, &ready_task_ids);
    Ok(ready_task_ids)
}

fn schedule_task_ready_events<B: MacroEventBroker>(event_broker: &B, task_ids: &[Uuid]) {
    for &task_id in task_ids {
        let event = PropertyMacroEvent::task_ready(TaskReadyMetadata { task_id });
        drop(event_broker.send_event(&event).inspect_err(|error| {
            tracing::error!(error = ?error, task_id = %task_id, "failed to schedule task readiness event");
        }));
    }
}

#[cfg(test)]
mod test;
