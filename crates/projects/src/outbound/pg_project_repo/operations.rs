use std::str::FromStr;

use macro_user_id::{cowlike::CowLike, user_id::MacroUserIdStr};
use sqlx::PgPool;

use crate::domain::models::{ProjectOperationalStatus, ProjectOperations, ProjectPriority};

pub(super) async fn get_project_operations(
    pool: &PgPool,
    project_id: &str,
) -> Result<Option<ProjectOperations>, sqlx::Error> {
    sqlx::query!(
        r#"
        SELECT
            operations.project_id,
            operations.status,
            operations.priority,
            operations.lead_user_id,
            operations.start_date,
            operations.target_date,
            operations.completed_at,
            operations.created_at,
            operations.updated_at,
            operations.policy
        FROM project_operations operations
        JOIN "Project" project ON project.id = operations.project_id
        WHERE operations.project_id = $1
          AND project."deletedAt" IS NULL
        "#,
        project_id,
    )
    .try_map(|row| {
        let lead_user_id = row
            .lead_user_id
            .as_deref()
            .map(MacroUserIdStr::parse_from_str)
            .transpose()
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?
            .map(CowLike::into_owned);

        Ok(ProjectOperations {
            project_id: row.project_id,
            status: ProjectOperationalStatus::from_str(&row.status)
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
            priority: ProjectPriority::from_str(&row.priority)
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
            lead_user_id,
            start_date: row.start_date,
            target_date: row.target_date,
            completed_at: row.completed_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
            policy: row.policy,
        })
    })
    .fetch_optional(pool)
    .await
}
