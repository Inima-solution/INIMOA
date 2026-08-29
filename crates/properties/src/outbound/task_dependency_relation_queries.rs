//! One-snapshot canonical direct task dependency relation query.

use std::collections::HashSet;

use models_properties::service::property_value::PropertyValue;
use models_properties::{EntityReference, EntityType};
use sqlx::{Pool, Postgres};
use system_properties::{StatusOption, SystemPropertyKey};
use uuid::Uuid;

use crate::domain::model::{
    TaskDependencyReadiness, TaskDependencyRelationsSnapshot, TaskReadiness,
};

pub async fn get_task_dependency_relations(
    pool: &Pool<Postgres>,
    task_ids: &[Uuid],
) -> anyhow::Result<Option<Vec<TaskDependencyRelationsSnapshot>>> {
    let rows = sqlx::query!(
        r#"
        WITH requested AS (
            SELECT task_id, ordinal FROM unnest($1::uuid[]) WITH ORDINALITY AS input(task_id, ordinal)
        ), source_tasks AS (
            SELECT requested.task_id, requested.ordinal, source."projectId" AS project_id,
                source.id IS NOT NULL AND source_type.document_id IS NOT NULL AS source_available,
                dependency_property.values AS dependency_values
            FROM requested
            LEFT JOIN "Document" source ON source.id = requested.task_id::text AND source."deletedAt" IS NULL
            LEFT JOIN document_sub_type source_type ON source_type.document_id = source.id AND source_type.sub_type = 'task'::document_sub_type_value
            LEFT JOIN entity_properties dependency_property ON dependency_property.entity_id = source.id AND dependency_property.entity_type = 'TASK' AND dependency_property.property_definition_id = $2
        ), expanded_refs AS (
            SELECT source_tasks.task_id, source_tasks.ordinal, refs.reference, refs.reference_ordinal,
                dependency_values IS NOT NULL AND NOT COALESCE(jsonb_typeof(dependency_values) = 'object' AND dependency_values->>'type' = 'EntityReference' AND jsonb_typeof(dependency_values->'value') = 'array', FALSE) AS malformed_value
            FROM source_tasks
            LEFT JOIN LATERAL jsonb_array_elements(CASE WHEN dependency_values IS NOT NULL AND jsonb_typeof(dependency_values) = 'object' AND dependency_values->>'type' = 'EntityReference' AND jsonb_typeof(dependency_values->'value') = 'array' THEN dependency_values->'value' ELSE '[]'::jsonb END) WITH ORDINALITY AS refs(reference, reference_ordinal) ON TRUE
        ), normalized_refs AS (
            SELECT task_id, ordinal, reference_ordinal, malformed_value, reference,
                CASE WHEN reference->>'entity_type' = 'TASK' AND reference->>'specific_message_id' IS NULL AND reference->>'entity_id' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' AND (reference->>'entity_id')::uuid <> task_id THEN (reference->>'entity_id')::uuid ELSE NULL END AS dependency_id
            FROM expanded_refs
        ), valid_refs AS (
            SELECT DISTINCT ON (task_id, ordinal, dependency_id) task_id, ordinal, dependency_id, reference_ordinal
            FROM normalized_refs WHERE dependency_id IS NOT NULL ORDER BY task_id, ordinal, dependency_id, reference_ordinal
        ), duplicate_refs AS (
            SELECT task_id, ordinal, dependency_id
            FROM normalized_refs
            WHERE dependency_id IS NOT NULL
            GROUP BY task_id, ordinal, dependency_id
            HAVING count(*) > 1
        ), classified_refs AS (
            SELECT valid_refs.task_id, valid_refs.ordinal, valid_refs.dependency_id, valid_refs.reference_ordinal,
                dependency.id IS NOT NULL AND dependency_type.document_id IS NOT NULL AS live_same_project_task,
                status.values = jsonb_build_object('type', 'SelectOption', 'value', jsonb_build_array($4::uuid)) AS completed
            FROM valid_refs
            JOIN source_tasks ON source_tasks.task_id = valid_refs.task_id AND source_tasks.ordinal = valid_refs.ordinal
            LEFT JOIN "Document" dependency ON dependency.id = valid_refs.dependency_id::text AND dependency."projectId" IS NOT DISTINCT FROM source_tasks.project_id AND dependency."deletedAt" IS NULL
            LEFT JOIN document_sub_type dependency_type ON dependency_type.document_id = dependency.id AND dependency_type.sub_type = 'task'::document_sub_type_value
            LEFT JOIN entity_properties status ON status.entity_id = dependency.id AND status.entity_type = 'TASK' AND status.property_definition_id = $3
        ), source_flags AS (
            SELECT source_tasks.task_id, source_tasks.ordinal,
                COALESCE(bool_or(normalized_refs.malformed_value), FALSE)
                  OR COALESCE(bool_or(normalized_refs.reference IS NOT NULL AND normalized_refs.dependency_id IS NULL), FALSE)
                  OR EXISTS (SELECT 1 FROM duplicate_refs WHERE duplicate_refs.task_id = source_tasks.task_id AND duplicate_refs.ordinal = source_tasks.ordinal) AS malformed
            FROM source_tasks LEFT JOIN normalized_refs ON normalized_refs.task_id = source_tasks.task_id AND normalized_refs.ordinal = source_tasks.ordinal
            GROUP BY source_tasks.task_id, source_tasks.ordinal
        ), reverse_candidates AS (
            SELECT source_tasks.task_id, source_tasks.ordinal, candidate.id, candidate."projectId" AS project_id,
                candidate."deletedAt" IS NULL AND candidate_type.document_id IS NOT NULL AS live_task,
                candidate_dependency.values AS dependency_values
            FROM source_tasks
            JOIN entity_properties candidate_dependency ON candidate_dependency.entity_type = 'TASK' AND candidate_dependency.property_definition_id = $2
            LEFT JOIN "Document" candidate ON candidate.id = candidate_dependency.entity_id
            LEFT JOIN document_sub_type candidate_type ON candidate_type.document_id = candidate.id AND candidate_type.sub_type = 'task'::document_sub_type_value
            WHERE jsonb_path_exists(candidate_dependency.values, '$.** ? (@.entity_id == $id)', jsonb_build_object('id', to_jsonb(source_tasks.task_id::text)))
        ), reverse_ranked AS (
            SELECT reverse_candidates.*, row_number() OVER (
                PARTITION BY task_id, ordinal
                ORDER BY id ASC NULLS LAST, dependency_values::text ASC
            ) AS candidate_rank
            FROM reverse_candidates
        ), reverse_rows AS (
            SELECT task_id, ordinal, id, project_id, live_task, dependency_values
            FROM reverse_ranked
            WHERE candidate_rank <= 201
        )
        SELECT source_tasks.task_id AS "task_id!", source_tasks.source_available AS "source_available!", source_tasks.project_id,
            CASE WHEN COALESCE(source_flags.malformed, FALSE) OR COALESCE(bool_or(NOT classified_refs.live_same_project_task), FALSE) OR COALESCE(bool_or(classified_refs.live_same_project_task AND NOT COALESCE(classified_refs.completed, FALSE)), FALSE) THEN 'blocked' ELSE 'ready' END AS "readiness!",
            COALESCE(array_agg(classified_refs.dependency_id ORDER BY classified_refs.reference_ordinal) FILTER (WHERE classified_refs.live_same_project_task), ARRAY[]::uuid[]) AS "depends_on_task_ids!",
            COALESCE(array_agg(classified_refs.dependency_id ORDER BY classified_refs.reference_ordinal) FILTER (WHERE classified_refs.live_same_project_task AND NOT COALESCE(classified_refs.completed, FALSE)), ARRAY[]::uuid[]) AS "blocking_task_ids!",
            COALESCE(source_flags.malformed, FALSE) OR COALESCE(bool_or(NOT classified_refs.live_same_project_task), FALSE) AS "has_unavailable_dependencies!",
            COALESCE((SELECT jsonb_agg(jsonb_build_object('id', reverse_rows.id, 'projectId', reverse_rows.project_id, 'liveTask', reverse_rows.live_task, 'dependencyValues', reverse_rows.dependency_values) ORDER BY reverse_rows.id ASC NULLS LAST, reverse_rows.dependency_values::text ASC) FROM reverse_rows WHERE reverse_rows.task_id = source_tasks.task_id AND reverse_rows.ordinal = source_tasks.ordinal), '[]'::jsonb) AS "reverse_rows!"
        FROM source_tasks
        LEFT JOIN source_flags ON source_flags.task_id = source_tasks.task_id AND source_flags.ordinal = source_tasks.ordinal
        LEFT JOIN classified_refs ON classified_refs.task_id = source_tasks.task_id AND classified_refs.ordinal = source_tasks.ordinal
        GROUP BY source_tasks.task_id, source_tasks.ordinal, source_tasks.source_available, source_tasks.project_id, source_flags.malformed
        ORDER BY source_tasks.ordinal
        "#,
        task_ids,
        SystemPropertyKey::DEPENDS_ON_UUID,
        SystemPropertyKey::STATUS_UUID,
        StatusOption::COMPLETED_UUID,
    ).fetch_all(pool).await?;

    if rows.iter().any(|row| !row.source_available) {
        return Ok(None);
    }
    Ok(Some(
        rows.into_iter()
            .map(|row| {
                let (successor_task_ids, has_unavailable_successors) =
                    successors_from_rows(row.task_id, row.project_id.as_deref(), row.reverse_rows);
                TaskDependencyRelationsSnapshot {
                    readiness: TaskDependencyReadiness {
                        task_id: row.task_id,
                        readiness: if row.readiness == "ready" {
                            TaskReadiness::Ready
                        } else {
                            TaskReadiness::Blocked
                        },
                        depends_on_task_ids: row.depends_on_task_ids,
                        blocking_task_ids: row.blocking_task_ids,
                        has_unavailable_dependencies: row.has_unavailable_dependencies,
                    },
                    successor_task_ids,
                    has_unavailable_successors,
                }
            })
            .collect(),
    ))
}

fn successors_from_rows(
    source: Uuid,
    project_id: Option<&str>,
    rows: serde_json::Value,
) -> (Vec<Uuid>, bool) {
    let Some(rows) = rows.as_array() else {
        return (Vec::new(), true);
    };
    if rows.len() > 200 {
        return (Vec::new(), true);
    }
    let mut successors = HashSet::new();
    let mut unavailable = false;
    for row in rows {
        let candidate = row
            .get("id")
            .and_then(serde_json::Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok());
        let mentions = row
            .get("dependencyValues")
            .is_some_and(|value| json_has_entity_id(value, source));
        if !mentions {
            continue;
        }
        let values = row
            .get("dependencyValues")
            .cloned()
            .and_then(|value| serde_json::from_value::<PropertyValue>(value).ok());
        let exact = values
            .and_then(|value| match value {
                PropertyValue::EntityRef(refs) => Some(refs),
                _ => None,
            })
            .is_some_and(|refs| {
                valid_references(&refs, candidate).is_some_and(|ids| ids.contains(&source))
            });
        if let (Some(candidate), true) = (
            candidate,
            row.get("liveTask").and_then(serde_json::Value::as_bool) == Some(true),
        ) && row.get("projectId").and_then(serde_json::Value::as_str) == project_id
            && exact
        {
            successors.insert(candidate);
        } else {
            unavailable = true;
        }
    }
    let mut successors = successors.into_iter().collect::<Vec<_>>();
    successors.sort_unstable();
    if successors.len() > 200 {
        return (Vec::new(), true);
    }
    (successors, unavailable)
}

fn valid_references(refs: &[EntityReference], source: Option<Uuid>) -> Option<HashSet<Uuid>> {
    let mut result = HashSet::new();
    for reference in refs {
        if reference.entity_type != EntityType::Task || reference.specific_message_id.is_some() {
            return None;
        }
        let id = Uuid::parse_str(&reference.entity_id).ok()?;
        if Some(id) == source || !result.insert(id) {
            return None;
        }
    }
    Some(result)
}

fn json_has_entity_id(value: &serde_json::Value, id: Uuid) -> bool {
    match value {
        serde_json::Value::Array(values) => {
            values.iter().any(|value| json_has_entity_id(value, id))
        }
        serde_json::Value::Object(values) => {
            values.get("entity_id").and_then(serde_json::Value::as_str)
                == Some(id.to_string().as_str())
                || values.values().any(|value| json_has_entity_id(value, id))
        }
        _ => false,
    }
}

#[cfg(test)]
mod test;
