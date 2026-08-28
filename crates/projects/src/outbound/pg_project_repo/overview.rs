use std::str::FromStr;

use macro_user_id::{cowlike::CowLike, user_id::MacroUserIdStr};
use model::project::Project;
use sqlx::PgPool;

use crate::domain::models::{
    ProjectOperationalStatus, ProjectOperations, ProjectOverviewImmediateChildren,
    ProjectOverviewSnapshot, ProjectPriority,
};

/// Reads the canonical project, operations, and exact direct-child counts in one scoped query.
pub(super) async fn get_project_overview_scoped(
    pool: &PgPool,
    project_id: &str,
    team_id: uuid::Uuid,
) -> Result<Option<ProjectOverviewSnapshot>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT
            project.id,
            project.name,
            project."userId" AS "user_id",
            project."parentId" AS "parent_id",
            project."createdAt"::timestamptz AS "created_at",
            project."updatedAt"::timestamptz AS "updated_at",
            project."deletedAt"::timestamptz AS "deleted_at",
            operations.project_id,
            operations.status,
            operations.priority,
            CASE WHEN lead_membership.user_id IS NULL THEN NULL ELSE operations.lead_user_id END AS "lead_user_id?",
            operations.start_date,
            operations.target_date,
            operations.completed_at,
            operations.created_at AS "operations_created_at",
            operations.updated_at AS "operations_updated_at",
            operations.policy,
            (
                SELECT COUNT(*)
                FROM "Project" child_project
                WHERE child_project."parentId" = project.id
                  AND child_project."deletedAt" IS NULL
            ) AS "child_projects!",
            (
                SELECT COUNT(*)
                FROM "Document" task_document
                JOIN document_sub_type task_sub_type
                  ON task_sub_type.document_id = task_document.id
                WHERE task_document."projectId" = project.id
                  AND task_document."deletedAt" IS NULL
                  AND task_sub_type.sub_type = 'task'::document_sub_type_value
            ) AS "tasks!",
            (
                SELECT COUNT(*)
                FROM "Document" document
                LEFT JOIN document_sub_type document_sub_type
                  ON document_sub_type.document_id = document.id
                WHERE document."projectId" = project.id
                  AND document."deletedAt" IS NULL
                  AND document_sub_type.sub_type IS DISTINCT FROM 'task'::document_sub_type_value
            ) AS "non_task_documents!",
            (
                SELECT COUNT(*)
                FROM "Chat" chat
                WHERE chat."projectId" = project.id
                  AND chat."deletedAt" IS NULL
            ) AS "chats!"
        FROM "Project" project
        JOIN project_operations operations ON operations.project_id = project.id
        JOIN team_user owner_membership
          ON owner_membership.user_id = project."userId"
         AND owner_membership.team_id = $2
        LEFT JOIN team_user lead_membership
          ON lead_membership.user_id = operations.lead_user_id
         AND lead_membership.team_id = owner_membership.team_id
        WHERE project.id = $1
          AND project."deletedAt" IS NULL
        "#,
        project_id,
        team_id,
    )
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        let immediate_children = ProjectOverviewImmediateChildren {
            child_projects: row.child_projects,
            tasks: row.tasks,
            non_task_documents: row.non_task_documents,
            chats: row.chats,
        };
        Ok(ProjectOverviewSnapshot {
            project: Project {
                id: row.id,
                name: row.name,
                user_id: row.user_id,
                parent_id: row.parent_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
                deleted_at: row.deleted_at,
            },
            operations: ProjectOperations {
                project_id: row.project_id,
                status: ProjectOperationalStatus::from_str(&row.status)
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
                priority: ProjectPriority::from_str(&row.priority)
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
                lead_user_id: row
                    .lead_user_id
                    .as_deref()
                    .map(MacroUserIdStr::parse_from_str)
                    .transpose()
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?
                    .map(CowLike::into_owned),
                start_date: row.start_date,
                target_date: row.target_date,
                completed_at: row.completed_at,
                created_at: row.operations_created_at,
                updated_at: row.operations_updated_at,
                policy: row.policy,
            },
            immediate_children,
        })
    })
    .transpose()
}
