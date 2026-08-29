//! Canonical task Status transition guard.
use crate::domain::model::{
    EntityPropertyMutationSnapshot, TaskDependencyReadiness, TaskReadiness,
    TaskStatusMutationOutcome,
};
use models_properties::service::{entity_property::EntityProperty, property_value::PropertyValue};
use models_properties::{EntityReference, EntityType};
use sqlx::{Pool, Postgres, Transaction};
use system_properties::{
    StatusOption, SystemPropertyKey, outbound::task_readiness::final_ready_dependents,
};
use uuid::Uuid;

/// Capture the dependency facts visible to the guarded transaction. This is
/// deliberately data-only: caller-specific document receipts are minted only
/// after this transaction has ended.
pub(crate) fn malformed_dependency_readiness(task_id: Uuid) -> TaskDependencyReadiness {
    TaskDependencyReadiness {
        task_id,
        readiness: TaskReadiness::Blocked,
        depends_on_task_ids: Vec::new(),
        blocking_task_ids: Vec::new(),
        has_unavailable_dependencies: true,
    }
}

pub(crate) async fn task_dependency_readiness_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
    project_id: Option<&str>,
    dependency_ids: &[Uuid],
) -> anyhow::Result<TaskDependencyReadiness> {
    if dependency_ids.is_empty() {
        return Ok(TaskDependencyReadiness {
            task_id,
            readiness: TaskReadiness::Ready,
            depends_on_task_ids: Vec::new(),
            blocking_task_ids: Vec::new(),
            has_unavailable_dependencies: false,
        });
    }
    let ids = dependency_ids
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>();
    let rows = sqlx::query!(r#"WITH candidate AS (SELECT id, ord FROM UNNEST($1::text[]) WITH ORDINALITY AS candidate(id, ord)) SELECT candidate.id AS candidate_id, live.id AS live_id, ep.values AS status_values FROM candidate LEFT JOIN (SELECT d.id FROM "Document" d JOIN document_sub_type dst ON dst.document_id=d.id AND dst.sub_type='task' WHERE d."deletedAt" IS NULL AND d."projectId" IS NOT DISTINCT FROM $2) live ON live.id=candidate.id LEFT JOIN entity_properties ep ON ep.entity_id=live.id AND ep.entity_type='TASK' AND ep.property_definition_id=$3 ORDER BY candidate.ord"#, &ids, project_id, SystemPropertyKey::STATUS_UUID).fetch_all(&mut **tx).await?;
    let live = rows.into_iter().filter_map(|row| {
            let id = Uuid::parse_str(row.live_id?.as_str()).ok()?;
            let completed = row.status_values
                .and_then(|value| serde_json::from_value::<PropertyValue>(value).ok())
                .is_some_and(|value| matches!(value, PropertyValue::SelectOption(ids) if ids == vec![StatusOption::COMPLETED_UUID]));
            Some((id, completed))
        }).collect::<std::collections::HashMap<_, _>>();
    let mut depends_on_task_ids = Vec::new();
    let mut blocking_task_ids = Vec::new();
    let mut has_unavailable_dependencies = false;
    for id in dependency_ids {
        match live.get(id) {
            Some(completed) => {
                depends_on_task_ids.push(*id);
                if !completed {
                    blocking_task_ids.push(*id);
                }
            }
            None => has_unavailable_dependencies = true,
        }
    }
    Ok(TaskDependencyReadiness {
        task_id,
        readiness: if blocking_task_ids.is_empty() && !has_unavailable_dependencies {
            TaskReadiness::Ready
        } else {
            TaskReadiness::Blocked
        },
        depends_on_task_ids,
        blocking_task_ids,
        has_unavailable_dependencies,
    })
}

pub(crate) struct TaskGuardState {
    pub project_id: Option<String>,
    pub status: Option<StatusOption>,
    pub dependencies: Result<Vec<Uuid>, ()>,
}
pub(crate) fn requires_readiness(status: Option<StatusOption>) -> bool {
    matches!(
        status,
        Some(StatusOption::InProgress | StatusOption::InReview | StatusOption::Completed)
    )
}

pub(crate) async fn load_task_guard_state(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
) -> anyhow::Result<Option<TaskGuardState>> {
    let row = sqlx::query!(r#"SELECT d."projectId" AS project_id, status_ep.values AS status_values, depends_ep.values AS depends_values FROM "Document" d JOIN document_sub_type dst ON dst.document_id=d.id AND dst.sub_type='task' LEFT JOIN entity_properties status_ep ON status_ep.entity_id=d.id AND status_ep.entity_type='TASK' AND status_ep.property_definition_id=$2 LEFT JOIN entity_properties depends_ep ON depends_ep.entity_id=d.id AND depends_ep.entity_type='TASK' AND depends_ep.property_definition_id=$3 WHERE d.id=$1 AND d."deletedAt" IS NULL FOR UPDATE OF d"#, task_id.to_string(), SystemPropertyKey::STATUS_UUID, SystemPropertyKey::DEPENDS_ON_UUID).fetch_optional(&mut **tx).await?;
    Ok(row.map(|row| TaskGuardState {
        project_id: row.project_id,
        status: row
            .status_values
            .and_then(|v| serde_json::from_value::<PropertyValue>(v).ok())
            .and_then(|v| match v {
                PropertyValue::SelectOption(ids) if ids.len() == 1 => {
                    StatusOption::from_uuid(ids[0])
                }
                _ => None,
            }),
        dependencies: parse_dependencies(row.depends_values, task_id),
    }))
}
fn parse_dependencies(value: Option<serde_json::Value>, source: Uuid) -> Result<Vec<Uuid>, ()> {
    let Some(value) = value else {
        return Ok(vec![]);
    };
    let PropertyValue::EntityRef(refs) = serde_json::from_value(value).map_err(|_| ())? else {
        return Err(());
    };
    refs.into_iter()
        .map(|r| parse_reference(r, source))
        .collect()
}
fn parse_reference(r: EntityReference, source: Uuid) -> Result<Uuid, ()> {
    if r.entity_type != EntityType::Task || r.specific_message_id.is_some() {
        return Err(());
    };
    let id = Uuid::parse_str(&r.entity_id).map_err(|_| ())?;
    if id == source { Err(()) } else { Ok(id) }
}

pub async fn transition_task_status(
    pool: &Pool<Postgres>,
    task_id: Uuid,
    status: Option<StatusOption>,
) -> anyhow::Result<TaskStatusMutationOutcome> {
    let mut tx = pool.begin().await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(&mut *tx)
    .await?;
    let Some(state) = load_task_guard_state(&mut tx, task_id).await? else {
        return Ok(TaskStatusMutationOutcome::Blocked);
    };
    if requires_readiness(status) {
        let readiness = match &state.dependencies {
            Ok(ids) => {
                task_dependency_readiness_snapshot(
                    &mut tx,
                    task_id,
                    state.project_id.as_deref(),
                    ids,
                )
                .await?
            }
            Err(()) => malformed_dependency_readiness(task_id),
        };
        if readiness.readiness == TaskReadiness::Blocked {
            return Ok(TaskStatusMutationOutcome::BlockedWithReadiness(readiness));
        }
    };
    let value = status
        .map(|s| serde_json::to_value(PropertyValue::SelectOption(vec![s.uuid()])))
        .transpose()?;
    let row=sqlx::query!(r#"WITH previous AS (SELECT values FROM entity_properties WHERE entity_id=$2 AND entity_type='TASK' AND property_definition_id=$3) INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4) ON CONFLICT (entity_id,entity_type,property_definition_id) DO UPDATE SET values=EXCLUDED.values,updated_at=NOW() RETURNING id,entity_id,property_definition_id,values,(SELECT values FROM previous) AS previous,created_at,updated_at"#,macro_uuid::generate_uuid_v7(),task_id.to_string(),SystemPropertyKey::STATUS_UUID,value).fetch_one(&mut *tx).await?;
    let value = row
        .values
        .filter(|v| !v.is_null())
        .map(serde_json::from_value)
        .transpose()?;
    let previous_value = row
        .previous
        .filter(|v| !v.is_null())
        .and_then(|v| serde_json::from_value(v).ok());
    let snapshot = EntityPropertyMutationSnapshot {
        property: EntityProperty {
            id: row.id,
            entity_id: row.entity_id,
            entity_type: EntityType::Task,
            property_definition_id: row.property_definition_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        },
        value,
        previous_value,
    };
    let ready_task_ids = if status == Some(StatusOption::Completed)
        && state.status != Some(StatusOption::Completed)
    {
        final_ready_dependents(&mut tx, &[task_id], state.project_id.as_deref(), &[]).await?
    } else {
        Vec::new()
    };
    tx.commit().await?;
    if ready_task_ids.is_empty() {
        Ok(TaskStatusMutationOutcome::Updated(snapshot))
    } else {
        Ok(TaskStatusMutationOutcome::UpdatedWithReady {
            snapshot,
            ready_task_ids,
        })
    }
}
#[cfg(test)]
mod test;
