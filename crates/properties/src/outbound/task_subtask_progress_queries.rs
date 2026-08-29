//! One-snapshot canonical direct-subtask progress query.

use std::collections::HashSet;

use models_properties::service::property_value::PropertyValue;
use models_properties::{EntityReference, EntityType};
use sqlx::{Pool, Postgres};
use system_properties::{StatusOption, SystemPropertyKey};
use uuid::Uuid;

use crate::domain::model::TaskSubtaskProgressSnapshot;

/// Return one ordered snapshot per requested source, or `None` when any source
/// is not a live Task. All hierarchy interpretation stays in this module so
/// the public DTO cannot accidentally acquire child identities.
pub async fn get_task_subtask_progress(
    pool: &Pool<Postgres>,
    task_ids: &[Uuid],
) -> anyhow::Result<Option<Vec<TaskSubtaskProgressSnapshot>>> {
    let rows = sqlx::query!(
        r#"
        WITH requested AS (
            SELECT task_id, ordinal
            FROM unnest($1::uuid[]) WITH ORDINALITY AS input(task_id, ordinal)
        ), source AS (
            SELECT requested.task_id, requested.ordinal, document."projectId" AS project_id,
                document.id IS NOT NULL AND source_type.document_id IS NOT NULL AS source_available,
                subtasks.values AS subtasks_values
            FROM requested
            LEFT JOIN "Document" document ON document.id = requested.task_id::text AND document."deletedAt" IS NULL
            LEFT JOIN document_sub_type source_type ON source_type.document_id = document.id AND source_type.sub_type = 'task'::document_sub_type_value
            LEFT JOIN entity_properties subtasks ON subtasks.entity_id = document.id AND subtasks.entity_type = 'TASK' AND subtasks.property_definition_id = $2
        ), candidates AS (
            SELECT source.task_id, source.ordinal, source.project_id, refs.reference, refs.reference_ordinal
            FROM source
            LEFT JOIN LATERAL jsonb_array_elements(CASE
                WHEN jsonb_typeof(source.subtasks_values) = 'object'
                 AND source.subtasks_values->>'type' = 'EntityReference'
                 AND jsonb_typeof(source.subtasks_values->'value') = 'array'
                THEN source.subtasks_values->'value' ELSE '[]'::jsonb END
            ) WITH ORDINALITY AS refs(reference, reference_ordinal) ON TRUE
        ), candidate_rows AS (
            SELECT candidates.task_id, candidates.ordinal, candidates.reference, candidates.reference_ordinal,
                child.id AS child_id, status.values AS status_values
            FROM candidates
            LEFT JOIN "Document" child ON child.id = candidates.reference->>'entity_id'
                AND child."deletedAt" IS NULL AND child."projectId" IS NOT DISTINCT FROM candidates.project_id
            LEFT JOIN document_sub_type child_type ON child_type.document_id = child.id AND child_type.sub_type = 'task'::document_sub_type_value
            LEFT JOIN entity_properties status ON status.entity_id = child.id AND status.entity_type = 'TASK' AND status.property_definition_id = $4
            WHERE child_type.document_id IS NOT NULL OR candidates.reference IS NOT NULL
        ), reverse_rows AS (
            SELECT source.task_id, source.ordinal, child.id, parent.values AS parent_values
            FROM source
            JOIN "Document" child ON child."deletedAt" IS NULL AND child."projectId" IS NOT DISTINCT FROM source.project_id
            JOIN document_sub_type child_type ON child_type.document_id = child.id AND child_type.sub_type = 'task'::document_sub_type_value
            JOIN entity_properties parent ON parent.entity_id = child.id AND parent.entity_type = 'TASK' AND parent.property_definition_id = $3
        )
        SELECT source.task_id AS "task_id!", source.source_available AS "source_available!", source.subtasks_values,
            COALESCE((SELECT jsonb_agg(jsonb_build_object('reference', candidate_rows.reference, 'childId', candidate_rows.child_id, 'statusValues', candidate_rows.status_values) ORDER BY candidate_rows.reference_ordinal) FROM candidate_rows WHERE candidate_rows.task_id = source.task_id AND candidate_rows.ordinal = source.ordinal), '[]'::jsonb) AS "candidate_rows!",
            COALESCE((SELECT jsonb_agg(jsonb_build_object('id', reverse_rows.id, 'parentValues', reverse_rows.parent_values)) FROM reverse_rows WHERE reverse_rows.task_id = source.task_id AND reverse_rows.ordinal = source.ordinal), '[]'::jsonb) AS "reverse_rows!"
        FROM source
        GROUP BY source.task_id, source.ordinal, source.source_available, source.subtasks_values
        ORDER BY source.ordinal
        "#,
        task_ids,
        SystemPropertyKey::SUBTASKS_UUID,
        SystemPropertyKey::PARENT_TASK_UUID,
        SystemPropertyKey::STATUS_UUID,
    )
    .fetch_all(pool)
    .await?;
    if rows.iter().any(|row| !row.source_available) {
        return Ok(None);
    }
    let snapshots = rows
        .into_iter()
        .map(|row| {
            snapshot_from_row(
                row.task_id,
                row.subtasks_values,
                row.candidate_rows,
                row.reverse_rows,
            )
        })
        .collect();
    Ok(Some(snapshots))
}

fn snapshot_from_row(
    task_id: Uuid,
    subtasks_values: Option<serde_json::Value>,
    candidate_rows: serde_json::Value,
    reverse_rows: serde_json::Value,
) -> TaskSubtaskProgressSnapshot {
    let Ok(subtask_ids) = parse_subtasks(subtasks_values, task_id) else {
        return unavailable(task_id);
    };
    let mut unavailable_edge = false;
    let mut reverse_ids = HashSet::new();
    for reverse in reverse_rows.as_array().into_iter().flatten() {
        let child_id = reverse
            .get("id")
            .and_then(serde_json::Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok());
        let parent_value = reverse
            .get("parentValues")
            .cloned()
            .filter(|value| !value.is_null());
        let parent = parent_value
            .clone()
            .and_then(|value| serde_json::from_value::<PropertyValue>(value).ok());
        let mentions_source = match parent.as_ref() {
            Some(PropertyValue::EntityRef(references)) => references
                .iter()
                .any(|reference| Uuid::parse_str(&reference.entity_id).ok() == Some(task_id)),
            None => parent_value
                .as_ref()
                .is_some_and(|value| json_has_entity_id(value, task_id)),
            _ => false,
        };
        if mentions_source {
            let exact = matches!(parent.as_ref(), Some(PropertyValue::EntityRef(references)) if references.len() == 1 && child_id.is_some_and(|child_id| parse_reference(references[0].clone(), child_id).ok() == Some(task_id)));
            if let (true, Some(child_id)) = (exact, child_id) {
                reverse_ids.insert(child_id);
            } else {
                unavailable_edge = true;
            }
        }
    }
    let source_ids = subtask_ids.iter().copied().collect::<HashSet<_>>();
    if source_ids != reverse_ids {
        unavailable_edge = true;
    }
    let mut canonical = Vec::new();
    let mut completed = Vec::new();
    let mut canceled = Vec::new();
    for candidate in candidate_rows.as_array().into_iter().flatten() {
        let reference = candidate
            .get("reference")
            .cloned()
            .and_then(|value| serde_json::from_value::<EntityReference>(value).ok());
        let Some(reference) = reference else {
            unavailable_edge = true;
            continue;
        };
        let Ok(child_id) = parse_reference(reference, task_id) else {
            unavailable_edge = true;
            continue;
        };
        if candidate.get("childId").and_then(serde_json::Value::as_str)
            != Some(child_id.to_string().as_str())
            || !reverse_ids.contains(&child_id)
        {
            unavailable_edge = true;
            continue;
        }
        canonical.push(child_id);
        let status = candidate
            .get("statusValues")
            .cloned()
            .filter(|value| !value.is_null())
            .and_then(|value| serde_json::from_value::<PropertyValue>(value).ok())
            .and_then(|value| match value {
                PropertyValue::SelectOption(ids) if ids.len() == 1 => {
                    StatusOption::from_uuid(ids[0])
                }
                _ => None,
            });
        match status {
            Some(StatusOption::Completed) => completed.push(child_id),
            Some(StatusOption::Canceled) => canceled.push(child_id),
            _ => {}
        }
    }
    if canonical != subtask_ids {
        unavailable_edge = true;
    }
    TaskSubtaskProgressSnapshot {
        task_id,
        subtask_ids: canonical,
        completed_subtask_ids: completed,
        canceled_subtask_ids: canceled,
        has_unavailable_subtasks: unavailable_edge,
    }
}

fn unavailable(task_id: Uuid) -> TaskSubtaskProgressSnapshot {
    TaskSubtaskProgressSnapshot {
        task_id,
        subtask_ids: Vec::new(),
        completed_subtask_ids: Vec::new(),
        canceled_subtask_ids: Vec::new(),
        has_unavailable_subtasks: true,
    }
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
    let mut seen = HashSet::with_capacity(references.len());
    for reference in references {
        let id = parse_reference(reference, source)?;
        if !seen.insert(id) {
            return Err(());
        }
        ids.push(id);
    }
    Ok(ids)
}

fn parse_reference(reference: EntityReference, source: Uuid) -> Result<Uuid, ()> {
    if reference.entity_type != EntityType::Task || reference.specific_message_id.is_some() {
        return Err(());
    }
    let id = Uuid::parse_str(&reference.entity_id).map_err(|_| ())?;
    (id != source).then_some(id).ok_or(())
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

#[cfg(test)]
mod test;
