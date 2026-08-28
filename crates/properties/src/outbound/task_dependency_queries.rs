//! Atomic task dependency replacement queries.

use models_properties::service::{entity_property::EntityProperty, property_value::PropertyValue};
use models_properties::{EntityReference, EntityType};
use sqlx::{Pool, Postgres};
use system_properties::SystemPropertyKey;
use uuid::Uuid;

use crate::domain::model::{EntityPropertyMutationSnapshot, TaskDependencyMutationOutcome};
use crate::outbound::task_status_transition_queries::{
    all_dependencies_completed, load_task_guard_state, requires_readiness,
};

pub async fn replace_task_dependencies(
    pool: &Pool<Postgres>,
    task_id: Uuid,
    dependency_ids: &[Uuid],
) -> anyhow::Result<TaskDependencyMutationOutcome> {
    let mut tx = pool.begin().await?;
    // ponytail: one global lock serializes dependency replacements; shard per project only if measured throughput requires it.
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(&mut *tx)
    .await?;

    let Some(state) = load_task_guard_state(&mut tx, task_id).await? else {
        return Ok(TaskDependencyMutationOutcome::Unavailable);
    };
    let mut locked_ids = dependency_ids
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>();
    locked_ids.push(task_id.to_string());
    locked_ids.sort_unstable();
    locked_ids.dedup();
    let live_tasks = sqlx::query!(
        r#"
        SELECT d.id
        FROM "Document" d
        JOIN document_sub_type dst ON dst.document_id = d.id AND dst.sub_type = 'task'
        WHERE d.id = ANY($1)
          AND d."deletedAt" IS NULL
          AND d."projectId" IS NOT DISTINCT FROM (
              SELECT "projectId" FROM "Document" WHERE id = $2
          )
        ORDER BY d.id
        FOR UPDATE
        "#,
        &locked_ids,
        task_id.to_string(),
    )
    .fetch_all(&mut *tx)
    .await?;
    if live_tasks.len() != locked_ids.len() {
        return Ok(TaskDependencyMutationOutcome::Unavailable);
    }

    // The same lock and completion policy as Status writes closes the race:
    // an already guarded source cannot replace Depends On with an incomplete
    // predecessor set. This is intentionally checked only at this commit.
    if requires_readiness(state.status) {
        if !all_dependencies_completed(&mut tx, state.project_id.as_deref(), dependency_ids).await?
        {
            return Ok(TaskDependencyMutationOutcome::Blocked);
        }
    }

    if !dependency_ids.is_empty() {
        let dependency_text = dependency_ids
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>();
        let has_cycle = sqlx::query_scalar!(
            r#"
            WITH RECURSIVE reachable(id) AS (
                SELECT edge->>'entity_id'
                FROM entity_properties ep
                CROSS JOIN LATERAL jsonb_array_elements(
                    CASE WHEN jsonb_typeof(ep.values->'value') = 'array'
                        THEN ep.values->'value' ELSE '[]'::jsonb END
                ) edge
                WHERE ep.entity_id = ANY($1)
                  AND ep.entity_type = 'TASK'
                  AND ep.property_definition_id = $2
                  AND edge->>'entity_type' = 'TASK'
                UNION
                SELECT edge->>'entity_id'
                FROM entity_properties ep
                JOIN reachable r ON ep.entity_id = r.id
                CROSS JOIN LATERAL jsonb_array_elements(
                    CASE WHEN jsonb_typeof(ep.values->'value') = 'array'
                        THEN ep.values->'value' ELSE '[]'::jsonb END
                ) edge
                WHERE ep.entity_type = 'TASK'
                  AND ep.property_definition_id = $2
                  AND edge->>'entity_type' = 'TASK'
            )
            SELECT EXISTS(SELECT 1 FROM reachable WHERE id = $3) AS "has_cycle!"
            "#,
            &dependency_text,
            SystemPropertyKey::DEPENDS_ON_UUID,
            task_id.to_string(),
        )
        .fetch_one(&mut *tx)
        .await?;
        if has_cycle {
            return Ok(TaskDependencyMutationOutcome::Cycle);
        }
    }

    let value = if dependency_ids.is_empty() {
        None
    } else {
        Some(serde_json::to_value(PropertyValue::EntityRef(
            dependency_ids
                .iter()
                .map(|id| EntityReference::new(id.to_string(), EntityType::Task))
                .collect(),
        ))?)
    };
    let row = sqlx::query!(
        r#"
        WITH previous AS (
            SELECT values FROM entity_properties
            WHERE entity_id = $2 AND entity_type = 'TASK' AND property_definition_id = $3
        )
        INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values)
        VALUES ($1, $2, 'TASK', $3, $4)
        ON CONFLICT (entity_id, entity_type, property_definition_id)
        DO UPDATE SET values = EXCLUDED.values, updated_at = NOW()
        RETURNING id, entity_id, property_definition_id, values,
            (SELECT values FROM previous) AS previous, created_at, updated_at
        "#,
        macro_uuid::generate_uuid_v7(),
        task_id.to_string(),
        SystemPropertyKey::DEPENDS_ON_UUID,
        value,
    )
    .fetch_one(&mut *tx)
    .await?;
    let value = row
        .values
        .filter(|value| !value.is_null())
        .map(serde_json::from_value)
        .transpose()?;
    let previous_value = row
        .previous
        .filter(|value| !value.is_null())
        .and_then(|value| serde_json::from_value(value).ok());
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
    tx.commit().await?;
    Ok(TaskDependencyMutationOutcome::Updated(snapshot))
}

#[cfg(test)]
mod test;
