//! Remove hierarchy edges made invalid by a restored task changing projects.

use sqlx::PgConnection;

use crate::SystemPropertyKey;

/// Clear every hierarchy edge incident to a restored task after its project
/// changes. Callers hold `TASKDEPS` followed by `TASKHIER` in this transaction.
pub async fn reconcile_relocated_task_hierarchy(
    transaction: &mut PgConnection,
    restored_task_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        WITH source AS (
            SELECT d.id
            FROM "Document" d
            JOIN document_sub_type dst
              ON dst.document_id = d.id AND dst.sub_type = 'task'
            WHERE d.id = $1
        ), replacement AS (
            SELECT ep.id,
                CASE WHEN ep.entity_id = source.id THEN NULL
                     ELSE COALESCE(
                        jsonb_set(
                            ep.values,
                            '{value}',
                            (
                                SELECT jsonb_agg(reference ORDER BY ordinality)
                                FROM jsonb_array_elements(ep.values->'value')
                                     WITH ORDINALITY refs(reference, ordinality)
                                WHERE reference->>'entity_id' IS DISTINCT FROM source.id
                            ),
                            false
                        ),
                        NULL
                     )
                END AS values
            FROM entity_properties ep
            CROSS JOIN source
            WHERE ep.entity_type = 'TASK'
              AND ep.property_definition_id IN ($2, $3)
              AND (
                  ep.entity_id = source.id
                  OR EXISTS (
                      SELECT 1
                      FROM jsonb_array_elements(
                          CASE WHEN jsonb_typeof(ep.values->'value') = 'array'
                               THEN ep.values->'value' ELSE '[]'::jsonb END
                      ) reference
                      WHERE reference->>'entity_id' = source.id
                  )
              )
        )
        UPDATE entity_properties ep
        SET values = replacement.values, updated_at = NOW()
        FROM replacement
        WHERE ep.id = replacement.id
          AND ep.values IS DISTINCT FROM replacement.values
        "#,
        restored_task_id,
        SystemPropertyKey::PARENT_TASK_UUID,
        SystemPropertyKey::SUBTASKS_UUID,
    )
    .execute(&mut *transaction)
    .await?;
    Ok(())
}

#[cfg(test)]
mod test;
