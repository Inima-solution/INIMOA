//! One-snapshot, scoped direct task-dependency readiness read query.

use sqlx::{Pool, Postgres};
use system_properties::{StatusOption, SystemPropertyKey};
use uuid::Uuid;

use crate::domain::model::{TaskDependencyReadiness, TaskReadiness};

/// Read direct dependency readiness for a live project in one derived team
/// scope. The SQL emits one null-task witness for an available project with no
/// requested live task, allowing the repository to distinguish that from an
/// unavailable project without a second statement.
pub async fn get_task_dependency_readiness(
    pool: &Pool<Postgres>,
    project_id: &str,
    team_id: Uuid,
    task_ids: &[Uuid],
) -> anyhow::Result<Option<Vec<TaskDependencyReadiness>>> {
    let rows = sqlx::query!(
        r#"
        WITH scoped_project AS (
            SELECT project.id
            FROM "Project" project
            JOIN team_user owner_membership
              ON owner_membership.user_id = project."userId"
             AND owner_membership.team_id = $2
            WHERE project.id = $1
              AND project."deletedAt" IS NULL
        ),
        requested AS (
            SELECT requested_id, requested_ordinal
            FROM unnest($3::uuid[]) WITH ORDINALITY AS input(requested_id, requested_ordinal)
        ),
        requested_dedup AS (
            SELECT DISTINCT ON (requested_id) requested_id, requested_ordinal
            FROM requested
            ORDER BY requested_id, requested_ordinal
        ),
        source_tasks AS (
            SELECT requested_dedup.requested_id AS task_id, requested_dedup.requested_ordinal
            FROM requested_dedup
            JOIN "Document" source ON source.id = requested_dedup.requested_id::text
            JOIN document_sub_type source_sub_type
              ON source_sub_type.document_id = source.id
             AND source_sub_type.sub_type = 'task'::document_sub_type_value
            JOIN scoped_project ON scoped_project.id = source."projectId"
            WHERE source."deletedAt" IS NULL
        ),
        expanded_refs AS (
            SELECT
                source_tasks.task_id,
                refs.reference,
                refs.reference_ordinal,
                dependency_property.values IS NOT NULL
                  AND NOT COALESCE((
                    jsonb_typeof(dependency_property.values) = 'object'
                    AND dependency_property.values->>'type' = 'EntityReference'
                    AND jsonb_typeof(dependency_property.values->'value') = 'array'
                  ), FALSE) AS malformed_value
            FROM source_tasks
            LEFT JOIN entity_properties dependency_property
              ON dependency_property.entity_id = source_tasks.task_id::text
             AND dependency_property.entity_type = 'TASK'
             AND dependency_property.property_definition_id = $4
            LEFT JOIN LATERAL jsonb_array_elements(
                CASE
                    WHEN dependency_property.values IS NOT NULL
                     AND jsonb_typeof(dependency_property.values) = 'object'
                     AND dependency_property.values->>'type' = 'EntityReference'
                     AND jsonb_typeof(dependency_property.values->'value') = 'array'
                    THEN dependency_property.values->'value'
                    ELSE '[]'::jsonb
                END
            ) WITH ORDINALITY AS refs(reference, reference_ordinal) ON TRUE
        ),
        normalized_refs AS (
            SELECT
                task_id,
                reference_ordinal,
                malformed_value,
                reference,
                CASE
                    WHEN reference->>'entity_type' = 'TASK'
                     AND reference->>'specific_message_id' IS NULL
                     AND reference->>'entity_id' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    THEN CASE
                        WHEN (reference->>'entity_id')::uuid <> task_id
                        THEN (reference->>'entity_id')::uuid
                        ELSE NULL
                    END
                    ELSE NULL
                END AS dependency_id
            FROM expanded_refs
        ),
        valid_refs AS (
            SELECT DISTINCT ON (task_id, dependency_id)
                task_id, dependency_id, reference_ordinal
            FROM normalized_refs
            WHERE dependency_id IS NOT NULL
            ORDER BY task_id, dependency_id, reference_ordinal
        ),
        classified_refs AS (
            SELECT
                valid_refs.task_id,
                valid_refs.dependency_id,
                valid_refs.reference_ordinal,
                dependency.id IS NOT NULL AND dependency_sub_type.document_id IS NOT NULL
                  AS live_same_project_task,
                status_property.values = jsonb_build_object(
                    'type', 'SelectOption',
                    'value', jsonb_build_array($6::uuid)
                ) AS completed
            FROM valid_refs
            LEFT JOIN "Document" dependency
              ON dependency.id = valid_refs.dependency_id::text
             AND dependency."projectId" = $1
             AND dependency."deletedAt" IS NULL
            LEFT JOIN document_sub_type dependency_sub_type
              ON dependency_sub_type.document_id = dependency.id
             AND dependency_sub_type.sub_type = 'task'::document_sub_type_value
            LEFT JOIN entity_properties status_property
              ON status_property.entity_id = dependency.id
             AND status_property.entity_type = 'TASK'
             AND status_property.property_definition_id = $5
        ),
        source_flags AS (
            SELECT
                source_tasks.task_id,
                COALESCE(bool_or(normalized_refs.malformed_value), FALSE)
                  OR COALESCE(bool_or(
                    normalized_refs.reference IS NOT NULL
                    AND normalized_refs.dependency_id IS NULL
                  ), FALSE) AS has_malformed_dependency
            FROM source_tasks
            LEFT JOIN normalized_refs ON normalized_refs.task_id = source_tasks.task_id
            GROUP BY source_tasks.task_id
        )
        SELECT
            source_tasks.task_id,
            CASE WHEN COALESCE(source_flags.has_malformed_dependency, FALSE)
                    OR COALESCE(bool_or(NOT classified_refs.live_same_project_task), FALSE)
                    OR COALESCE(bool_or(
                        classified_refs.live_same_project_task
                          AND NOT COALESCE(classified_refs.completed, FALSE)
                    ), FALSE)
                 THEN 'blocked' ELSE 'ready' END AS "readiness!",
            COALESCE(
                array_agg(classified_refs.dependency_id ORDER BY classified_refs.reference_ordinal)
                    FILTER (WHERE classified_refs.live_same_project_task),
                ARRAY[]::uuid[]
            ) AS "depends_on_task_ids!",
            COALESCE(
                array_agg(classified_refs.dependency_id ORDER BY classified_refs.reference_ordinal)
                    FILTER (
                        WHERE classified_refs.live_same_project_task
                          AND NOT COALESCE(classified_refs.completed, FALSE)
                    ),
                ARRAY[]::uuid[]
            ) AS "blocking_task_ids!",
            COALESCE(source_flags.has_malformed_dependency, FALSE)
              OR COALESCE(bool_or(NOT classified_refs.live_same_project_task), FALSE)
              AS "has_unavailable_dependencies!"
        FROM scoped_project
        LEFT JOIN source_tasks ON TRUE
        LEFT JOIN source_flags ON source_flags.task_id = source_tasks.task_id
        LEFT JOIN classified_refs ON classified_refs.task_id = source_tasks.task_id
        GROUP BY source_tasks.task_id, source_tasks.requested_ordinal, source_flags.has_malformed_dependency
        ORDER BY source_tasks.requested_ordinal
        "#,
        project_id,
        team_id,
        task_ids,
        SystemPropertyKey::DEPENDS_ON_UUID,
        SystemPropertyKey::STATUS_UUID,
        StatusOption::COMPLETED_UUID,
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(None);
    }

    Ok(Some(
        rows.into_iter()
            .filter_map(|row| {
                let task_id = row.task_id?;
                let readiness = match row.readiness.as_str() {
                    "ready" => TaskReadiness::Ready,
                    "blocked" => TaskReadiness::Blocked,
                    _ => unreachable!("static readiness query only returns known literals"),
                };
                Some(TaskDependencyReadiness {
                    task_id,
                    readiness,
                    depends_on_task_ids: row.depends_on_task_ids,
                    blocking_task_ids: row.blocking_task_ids,
                    has_unavailable_dependencies: row.has_unavailable_dependencies,
                })
            })
            .collect::<Vec<_>>(),
    ))
}

#[cfg(test)]
mod test;
