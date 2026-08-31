use sqlx::{PgPool, Row};
use system_properties::{StatusOption, SystemPropertyKey};

use crate::domain::models::ProjectTaskProgress;

/// Reads one project's direct task progress in one scoped aggregate query.
pub(super) async fn get_project_task_progress_scoped(
    pool: &PgPool,
    project_id: &str,
    team_id: uuid::Uuid,
) -> Result<Option<ProjectTaskProgress>, sqlx::Error> {
    let completed = StatusOption::COMPLETED_UUID.to_string();
    let canceled = StatusOption::CANCELED_UUID.to_string();
    let not_started = StatusOption::NOT_STARTED_UUID.to_string();
    let in_progress = StatusOption::IN_PROGRESS_UUID.to_string();
    let in_review = StatusOption::IN_REVIEW_UUID.to_string();
    let status_property = SystemPropertyKey::STATUS_UUID;
    let row = sqlx::query(
        r#"
        WITH scoped_project AS (
            SELECT project.id
            FROM "Project" project
            JOIN team_user owner_membership
              ON owner_membership.user_id = project."userId"
             AND owner_membership.team_id = $2
            WHERE project.id = $1
              AND project."deletedAt" IS NULL
        ), direct_tasks AS (
            SELECT status.id IS NOT NULL AS has_status, status.values
            FROM scoped_project project
            JOIN "Document" document
              ON document."projectId" = project.id
             AND document."deletedAt" IS NULL
            JOIN document_sub_type subtype
              ON subtype.document_id = document.id
             AND subtype.sub_type = 'task'::document_sub_type_value
            LEFT JOIN entity_properties status
              ON status.entity_id = document.id
             AND status.entity_type = 'TASK'
             AND status.property_definition_id = $3
        ), normalized AS (
            SELECT *,
              has_status AND values IS NOT NULL AND values <> 'null'::jsonb AS has_usable_value,
              CASE
                WHEN NOT (has_status AND values IS NOT NULL AND values <> 'null'::jsonb)
                  OR jsonb_typeof(values) <> 'object'
                  OR values->>'type' <> 'SelectOption'
                  OR jsonb_typeof(values->'value') <> 'array'
                THEN NULL
                WHEN jsonb_array_length(values->'value') <> 1 THEN NULL
                ELSE values->'value'->>0
              END AS option_id
            FROM direct_tasks
        ), classified AS (
            SELECT *, COALESCE(option_id IN ($4, $5, $6, $7, $8), false) AS is_exact_known
            FROM normalized
        )
        SELECT
          progress.included_tasks,
          progress.completed_tasks,
          progress.has_unavailable_statuses
        FROM scoped_project project
        CROSS JOIN LATERAL (
          SELECT
            COUNT(*) FILTER (WHERE NOT (is_exact_known AND option_id = $5)) AS included_tasks,
            COUNT(*) FILTER (WHERE is_exact_known AND option_id = $4) AS completed_tasks,
            COALESCE(BOOL_OR(has_usable_value AND NOT is_exact_known), false) AS has_unavailable_statuses
          FROM classified
        ) progress
        "#,
    )
    .bind(project_id)
    .bind(team_id)
    .bind(status_property)
    .bind(completed)
    .bind(canceled)
    .bind(not_started)
    .bind(in_progress)
    .bind(in_review)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        ProjectTaskProgress::new(
            row.try_get("completed_tasks")?,
            row.try_get("included_tasks")?,
            row.try_get("has_unavailable_statuses")?,
        )
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))
    })
    .transpose()
}
