//! Authoritative task-ready notification snapshot query.

use macro_user_id::{cowlike::CowLike, user_id::MacroUserIdStr};
use models_properties::{EntityReference, EntityType};
use sqlx::{Pool, Postgres, Transaction};
use system_properties::{StatusOption, SystemPropertyKey};
use uuid::Uuid;

use super::task_status_transition_queries::{
    load_task_guard_state, malformed_dependency_readiness, task_dependency_readiness_snapshot,
};
use crate::domain::model::TaskReadiness;

/// Current display data and canonical current assignees for one live task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskReadyNotificationSnapshot {
    pub task_name: String,
    pub recipient_ids: Vec<MacroUserIdStr<'static>>,
}

/// Atomically revalidates a task-ready event against the current task state
/// and returns only notification-safe current data. This deliberately shares
/// the TASKDEPS lock and readiness implementation with status transitions.
pub async fn load_current_task_ready_notification(
    pool: &Pool<Postgres>,
    task_id: Uuid,
) -> anyhow::Result<Option<TaskReadyNotificationSnapshot>> {
    let mut tx = pool.begin().await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(&mut *tx)
    .await?;

    let Some(state) = load_task_guard_state(&mut tx, task_id).await? else {
        return Ok(None);
    };
    if state.status == Some(StatusOption::Canceled) {
        return Ok(None);
    }
    let readiness = match &state.dependencies {
        Ok(ids) => {
            task_dependency_readiness_snapshot(&mut tx, task_id, state.project_id.as_deref(), ids)
                .await?
        }
        Err(()) => malformed_dependency_readiness(task_id),
    };
    if readiness.readiness != TaskReadiness::Ready {
        return Ok(None);
    }
    let snapshot = load_task_ready_notification_snapshot(&mut tx, task_id).await?;
    tx.commit().await?;
    Ok(snapshot.filter(|snapshot| !snapshot.recipient_ids.is_empty()))
}

/// Load task notification data inside the same guarded transaction that
/// validates readiness. Invalid references are deliberately omitted.
pub async fn load_task_ready_notification_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
) -> anyhow::Result<Option<TaskReadyNotificationSnapshot>> {
    let row = sqlx::query!(
        r#"SELECT d.name, assignees.values AS assignee_values
           FROM "Document" d
           JOIN document_sub_type dst ON dst.document_id = d.id AND dst.sub_type = 'task'
           LEFT JOIN entity_properties assignees
             ON assignees.entity_id = d.id
            AND assignees.entity_type = 'TASK'
            AND assignees.property_definition_id = $2
          WHERE d.id = $1 AND d."deletedAt" IS NULL"#,
        task_id.to_string(),
        SystemPropertyKey::ASSIGNEES_UUID,
    )
    .fetch_optional(&mut **tx)
    .await?;

    let Some(row) = row else { return Ok(None) };
    let recipient_ids = row
        .assignee_values
        .and_then(|value| {
            serde_json::from_value::<models_properties::service::property_value::PropertyValue>(
                value,
            )
            .ok()
        })
        .and_then(|value| match value {
            models_properties::service::property_value::PropertyValue::EntityRef(refs) => {
                Some(refs)
            }
            _ => None,
        })
        .unwrap_or_default()
        .into_iter()
        .filter_map(canonical_user_id)
        .fold(Vec::new(), |mut ids, id| {
            if !ids.contains(&id) {
                ids.push(id);
            }
            ids
        });

    Ok(Some(TaskReadyNotificationSnapshot {
        task_name: row.name,
        recipient_ids,
    }))
}

fn canonical_user_id(reference: EntityReference) -> Option<MacroUserIdStr<'static>> {
    if reference.entity_type != EntityType::User || reference.specific_message_id.is_some() {
        return None;
    }
    MacroUserIdStr::parse_from_str(&reference.entity_id)
        .ok()
        .map(|id| id.into_owned())
}

#[cfg(test)]
mod test;
