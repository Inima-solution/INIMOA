//! Canonical final-readiness reverse fanout query shared by lifecycle mutations.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{StatusOption, SystemPropertyKey};

/// Return every live, same-project task that is finally ready after one or
/// more predecessors become available.  Callers must hold the TASKDEPS
/// transaction lock while changing predecessor availability and running this
/// query.  `excluded_task_ids` prevents a restored source from notifying
/// itself (or another caller-owned source).
pub async fn final_ready_dependents(
    transaction: &mut Transaction<'_, Postgres>,
    newly_available_predecessor_ids: &[Uuid],
    project_id: Option<&str>,
    excluded_task_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if newly_available_predecessor_ids.is_empty() {
        return Ok(Vec::new());
    }
    let predecessor_ids = newly_available_predecessor_ids
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>();
    let excluded_task_ids = excluded_task_ids
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>();
    let ids = sqlx::query_scalar!(
        r#"
        WITH dependents AS (
            SELECT DISTINCT d.id, depends_ep.values
            FROM "Document" d
            JOIN document_sub_type dst
              ON dst.document_id = d.id AND dst.sub_type = 'task'
            JOIN entity_properties depends_ep
              ON depends_ep.entity_id = d.id
             AND depends_ep.entity_type = 'TASK'
             AND depends_ep.property_definition_id = $3
            CROSS JOIN LATERAL jsonb_array_elements(
                CASE WHEN jsonb_typeof(depends_ep.values->'value') = 'array'
                    THEN depends_ep.values->'value' ELSE '[]'::jsonb END
            ) predecessor_ref
            WHERE d."deletedAt" IS NULL
              AND d."projectId" IS NOT DISTINCT FROM $2
              AND jsonb_typeof(depends_ep.values) = 'object'
              AND depends_ep.values->>'type' = 'EntityReference'
              AND jsonb_typeof(depends_ep.values->'value') = 'array'
              AND predecessor_ref->>'entity_type' = 'TASK'
              AND predecessor_ref->>'entity_id' = ANY($1)
        )
        SELECT DISTINCT dependents.id AS "id!"
        FROM dependents
        WHERE dependents.id <> ALL($6)
          AND NOT EXISTS (
            SELECT 1
            FROM jsonb_array_elements(
                CASE WHEN jsonb_typeof(dependents.values->'value') = 'array'
                    THEN dependents.values->'value' ELSE '[]'::jsonb END
            ) dependency_ref
            LEFT JOIN "Document" predecessor
              ON dependency_ref->>'entity_id' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
             AND predecessor.id = dependency_ref->>'entity_id'
             AND predecessor."deletedAt" IS NULL
             AND predecessor."projectId" IS NOT DISTINCT FROM $2
            LEFT JOIN document_sub_type predecessor_sub_type
              ON predecessor_sub_type.document_id = predecessor.id
             AND predecessor_sub_type.sub_type = 'task'
            LEFT JOIN entity_properties predecessor_status
              ON predecessor_status.entity_id = predecessor.id
             AND predecessor_status.entity_type = 'TASK'
             AND predecessor_status.property_definition_id = $4
            WHERE jsonb_typeof(dependency_ref) <> 'object'
               OR dependency_ref->>'entity_type' IS DISTINCT FROM 'TASK'
               OR COALESCE(dependency_ref->'specific_message_id' <> 'null'::jsonb, false)
               OR dependency_ref->>'entity_id' = dependents.id
               OR predecessor.id IS NULL
               OR predecessor_sub_type.document_id IS NULL
               OR predecessor_status.values IS DISTINCT FROM jsonb_build_object(
                    'type', 'SelectOption', 'value', jsonb_build_array($5::uuid)
                  )
        )
        ORDER BY dependents.id
        "#,
        &predecessor_ids,
        project_id,
        SystemPropertyKey::DEPENDS_ON_UUID,
        SystemPropertyKey::STATUS_UUID,
        StatusOption::COMPLETED_UUID,
        &excluded_task_ids,
    )
    .fetch_all(transaction.as_mut())
    .await?;
    ids.into_iter()
        .map(|id| Uuid::parse_str(&id).map_err(|error| sqlx::Error::Decode(Box::new(error))))
        .collect()
}

#[cfg(test)]
mod test;
