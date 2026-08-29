//! Canonical task Status transition guard.
use crate::domain::model::{
    EntityPropertyMutationSnapshot, TaskDependencyReadiness, TaskReadiness,
    TaskStatusMutationOutcome, TaskSubtaskCompletionReadiness,
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

fn parse_subtasks(value: Option<serde_json::Value>, source: Uuid) -> Result<Vec<Uuid>, ()> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let PropertyValue::EntityRef(references) = serde_json::from_value(value).map_err(|_| ())?
    else {
        return Err(());
    };
    let mut ids = Vec::with_capacity(references.len());
    let mut seen = std::collections::HashSet::with_capacity(references.len());
    for reference in references {
        let id = parse_reference(reference, source)?;
        if !seen.insert(id) {
            return Err(());
        }
        ids.push(id);
    }
    Ok(ids)
}

fn malformed_subtask_completion_readiness(task_id: Uuid) -> TaskSubtaskCompletionReadiness {
    TaskSubtaskCompletionReadiness {
        task_id,
        readiness: TaskReadiness::Blocked,
        subtask_ids: Vec::new(),
        blocking_subtask_ids: Vec::new(),
        has_unavailable_subtasks: true,
    }
}

/// Read the canonical hierarchy while both transition locks are held. The
/// source list controls output order; any unavailable or one-sided edge is
/// fail-closed and deliberately omitted from the returned IDs.
async fn task_subtask_completion_readiness_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
    project_id: Option<&str>,
) -> anyhow::Result<TaskSubtaskCompletionReadiness> {
    let row = sqlx::query!(r#"
        WITH source AS (SELECT subtasks_ep.values AS subtasks_values FROM "Document" d LEFT JOIN entity_properties subtasks_ep ON subtasks_ep.entity_id=d.id AND subtasks_ep.entity_type='TASK' AND subtasks_ep.property_definition_id=$2 WHERE d.id=$1 AND d."deletedAt" IS NULL),
        candidates AS (SELECT elem.value AS reference, elem.ord FROM source, LATERAL jsonb_array_elements(CASE WHEN jsonb_typeof(source.subtasks_values->'value') = 'array' THEN source.subtasks_values->'value' ELSE '[]'::jsonb END) WITH ORDINALITY AS elem(value, ord)),
        candidate_rows AS (SELECT candidates.reference, candidates.ord, live.id AS live_id, status_ep.values AS status_values FROM candidates LEFT JOIN (SELECT d.id FROM "Document" d JOIN document_sub_type dst ON dst.document_id=d.id AND dst.sub_type='task' WHERE d."deletedAt" IS NULL AND d."projectId" IS NOT DISTINCT FROM $3) live ON live.id=candidates.reference->>'entity_id' LEFT JOIN entity_properties status_ep ON status_ep.entity_id=live.id AND status_ep.entity_type='TASK' AND status_ep.property_definition_id=$4),
        reverse_rows AS (SELECT d.id, parent_ep.values AS parent_values FROM "Document" d JOIN document_sub_type dst ON dst.document_id=d.id AND dst.sub_type='task' JOIN entity_properties parent_ep ON parent_ep.entity_id=d.id AND parent_ep.entity_type='TASK' AND parent_ep.property_definition_id=$5 WHERE d."deletedAt" IS NULL AND d."projectId" IS NOT DISTINCT FROM $3)
        SELECT source.subtasks_values, COALESCE((SELECT jsonb_agg(jsonb_build_object('reference', reference, 'liveId', live_id, 'statusValues', status_values) ORDER BY ord) FROM candidate_rows), '[]'::jsonb) AS "candidate_rows!", COALESCE((SELECT jsonb_agg(jsonb_build_object('id', id, 'parentValues', parent_values)) FROM reverse_rows), '[]'::jsonb) AS "reverse_rows!" FROM source
        "#, task_id.to_string(), SystemPropertyKey::SUBTASKS_UUID, project_id, SystemPropertyKey::STATUS_UUID, SystemPropertyKey::PARENT_TASK_UUID).fetch_one(&mut **tx).await?;
    let subtasks = match parse_subtasks(row.subtasks_values, task_id) {
        Ok(ids) => ids,
        Err(()) => return Ok(malformed_subtask_completion_readiness(task_id)),
    };
    let mut reverse_ids = std::collections::HashSet::new();
    let mut has_unavailable_subtasks = false;
    for reverse in row.reverse_rows.as_array().into_iter().flatten() {
        let parent_value = reverse
            .get("parentValues")
            .cloned()
            .filter(|value| !value.is_null());
        let parent = parent_value
            .clone()
            .and_then(|value| serde_json::from_value::<PropertyValue>(value).ok());
        let references_source = match parent.as_ref() {
            Some(PropertyValue::EntityRef(refs)) => refs
                .iter()
                .any(|reference| Uuid::parse_str(&reference.entity_id).ok() == Some(task_id)),
            None => parent_value
                .as_ref()
                .is_some_and(|value| json_has_entity_id(value, task_id)),
            _ => false,
        };
        if references_source {
            let child_id = reverse
                .get("id")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok());
            let exact = matches!(parent.as_ref(), Some(PropertyValue::EntityRef(refs)) if refs.len() == 1 && child_id.is_some_and(|child_id| parse_reference(refs[0].clone(), child_id).ok() == Some(task_id)));
            if let (true, Some(child_id)) = (exact, child_id) {
                reverse_ids.insert(child_id);
            } else {
                has_unavailable_subtasks = true;
            }
        }
    }
    let source_set = subtasks
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    if source_set != reverse_ids {
        has_unavailable_subtasks = true;
    }
    let mut subtask_ids = Vec::new();
    let mut blocking_subtask_ids = Vec::new();
    for candidate in row.candidate_rows.as_array().into_iter().flatten() {
        let Some(reference) = candidate
            .get("reference")
            .cloned()
            .and_then(|value| serde_json::from_value::<EntityReference>(value).ok())
        else {
            has_unavailable_subtasks = true;
            continue;
        };
        let Ok(id) = parse_reference(reference, task_id) else {
            has_unavailable_subtasks = true;
            continue;
        };
        let id_text = id.to_string();
        let reciprocal = candidate.get("liveId").and_then(serde_json::Value::as_str)
            == Some(id_text.as_str())
            && reverse_ids.contains(&id);
        if !reciprocal {
            has_unavailable_subtasks = true;
            continue;
        }
        subtask_ids.push(id);
        let terminal = candidate.get("statusValues").cloned().filter(|value| !value.is_null()).and_then(|value| serde_json::from_value::<PropertyValue>(value).ok())
            .is_some_and(|value| matches!(value, PropertyValue::SelectOption(ids) if ids == vec![StatusOption::COMPLETED_UUID] || ids == vec![StatusOption::CANCELED_UUID]));
        if !terminal {
            blocking_subtask_ids.push(id);
        }
    }
    Ok(TaskSubtaskCompletionReadiness {
        task_id,
        readiness: if blocking_subtask_ids.is_empty() && !has_unavailable_subtasks {
            TaskReadiness::Ready
        } else {
            TaskReadiness::Blocked
        },
        subtask_ids,
        blocking_subtask_ids,
        has_unavailable_subtasks,
    })
}

fn json_has_entity_id(value: &serde_json::Value, task_id: Uuid) -> bool {
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| json_has_entity_id(value, task_id)),
        serde_json::Value::Object(values) => {
            values.get("entity_id").and_then(serde_json::Value::as_str)
                == Some(task_id.to_string().as_str())
                || values
                    .values()
                    .any(|value| json_has_entity_id(value, task_id))
        }
        _ => false,
    }
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
    if status == Some(StatusOption::Completed) {
        sqlx::query_scalar!(
            r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
            i64::from_be_bytes(*b"TASKHIER")
        )
        .fetch_one(&mut *tx)
        .await?;
    }
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
    if status == Some(StatusOption::Completed) {
        let readiness = task_subtask_completion_readiness_snapshot(
            &mut tx,
            task_id,
            state.project_id.as_deref(),
        )
        .await?;
        if readiness.readiness == TaskReadiness::Blocked {
            return Ok(TaskStatusMutationOutcome::BlockedBySubtasks(readiness));
        }
    }
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
