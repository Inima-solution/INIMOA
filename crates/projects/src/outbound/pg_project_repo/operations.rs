use std::str::FromStr;

use business_audit::{
    Actor, AuditAction, AuditEvent, AuditOutcome, AuditTarget, ProjectOperationsUpdatedMetadata,
    RequestCorrelationId, RetentionClass, insert_with_tx,
};
use macro_user_id::{cowlike::CowLike, user_id::MacroUserIdStr};
use sqlx::PgPool;

use crate::domain::models::{
    ProjectOperationalStatus, ProjectOperations, ProjectPriority, UpdateProjectOperationsCommand,
    UpdateProjectOperationsOutcome,
};

macro_rules! project_operations_from_row {
    ($row:expr) => {{
        let row = $row;
        Ok::<ProjectOperations, sqlx::Error>(ProjectOperations {
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
            created_at: row.created_at,
            updated_at: row.updated_at,
            policy: row.policy,
        })
    }};
}

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
    .try_map(|row| project_operations_from_row!(row))
    .fetch_optional(pool)
    .await
}

/// Reads operations only when the active project owner's team is the supplied team.
pub(super) async fn get_project_operations_scoped(
    pool: &PgPool,
    project_id: &str,
    team_id: uuid::Uuid,
) -> Result<Option<ProjectOperations>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT operations.project_id, operations.status, operations.priority,
               CASE WHEN lead_membership.user_id IS NULL THEN NULL ELSE operations.lead_user_id END AS lead_user_id,
               operations.start_date, operations.target_date, operations.completed_at,
               operations.created_at, operations.updated_at, operations.policy
        FROM "Project" project
        JOIN project_operations operations ON operations.project_id = project.id
        JOIN team_user owner_membership
          ON owner_membership.user_id = project."userId" AND owner_membership.team_id = $2
        LEFT JOIN team_user lead_membership
          ON lead_membership.user_id = operations.lead_user_id AND lead_membership.team_id = owner_membership.team_id
        WHERE project.id = $1 AND project."deletedAt" IS NULL
        "#,
        project_id,
        team_id,
    )
    .fetch_optional(pool)
    .await?;
    row.map(|row| project_operations_from_row!(row)).transpose()
}

/// Replaces operations after locking the canonical project, operations row, and owner team.
pub(super) async fn update_project_operations(
    pool: &PgPool,
    command: UpdateProjectOperationsCommand,
) -> Result<UpdateProjectOperationsOutcome, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let Some(row) = sqlx::query!(
        r#"
        SELECT
            operations.project_id, operations.status, operations.priority,
            operations.lead_user_id, operations.start_date, operations.target_date,
            operations.completed_at, operations.created_at, operations.updated_at, operations.policy,
            owner_membership.team_id AS owner_team_id
        FROM "Project" project
        JOIN project_operations operations ON operations.project_id = project.id
        JOIN team_user owner_membership ON owner_membership.user_id = project."userId"
        WHERE project.id = $1 AND project."deletedAt" IS NULL
        FOR UPDATE OF project, operations, owner_membership
        "#,
        &command.request.project_id,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(UpdateProjectOperationsOutcome::NotFound);
    };

    let owner_team_id = row.owner_team_id;
    if owner_team_id != command.team_id {
        return Ok(UpdateProjectOperationsOutcome::NotFound);
    }
    let current = project_operations_from_row!(row)?;
    let resolved = match command
        .request
        .replacement
        .resolve(&current, command.request.now)
    {
        Ok(resolved) => resolved,
        Err(error) => return Ok(UpdateProjectOperationsOutcome::Invalid(error)),
    };
    if let Some(lead_user_id) = &resolved.lead_user_id {
        let active = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM team_user WHERE user_id = $1 AND team_id = $2)",
            lead_user_id.as_ref(),
            owner_team_id,
        )
        .fetch_one(&mut *tx)
        .await?;
        if active != Some(true) {
            return Ok(UpdateProjectOperationsOutcome::LeadNotInOwnerTeam);
        }
    }
    if resolved.changed_fields.is_empty() {
        return Ok(UpdateProjectOperationsOutcome::Unchanged(current));
    }
    if current.updated_at != command.request.replacement.expected_updated_at {
        return Ok(UpdateProjectOperationsOutcome::Conflict);
    }

    let updated = sqlx::query!(
        r#"
        UPDATE project_operations
        SET status = $2, priority = $3, lead_user_id = $4, start_date = $5, target_date = $6,
            completed_at = $7, policy = $8, updated_at = $9
        WHERE project_id = $1
        RETURNING project_id, status, priority, lead_user_id, start_date, target_date,
                  completed_at, created_at, updated_at, policy
        "#,
        &command.request.project_id,
        resolved.status.to_string(),
        resolved.priority.to_string(),
        resolved.lead_user_id.as_ref().map(ToString::to_string),
        resolved.start_date,
        resolved.target_date,
        resolved.completed_at,
        resolved.policy,
        command.request.now,
    )
    .fetch_one(&mut *tx)
    .await?;

    let request_id = RequestCorrelationId::try_new(command.request.request_id)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let audit = AuditEvent::new(
        owner_team_id,
        Actor::new_from_user(command.actor_user_id),
        None,
        AuditAction::ProjectOperationsUpdated(
            ProjectOperationsUpdatedMetadata::new(
                audit_status(current.status),
                audit_status(resolved.status),
                resolved.changed_fields.into_iter().map(audit_changed_field),
            )
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?,
        ),
        AuditTarget::Project(command.request.project_id),
        AuditOutcome::Success,
        command.request.now,
        request_id,
        None,
        RetentionClass::Standard,
    )
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    insert_with_tx(&mut tx, &audit).await?;
    tx.commit().await?;
    Ok(UpdateProjectOperationsOutcome::Updated(
        project_operations_from_row!(updated)?,
    ))
}

fn audit_status(status: ProjectOperationalStatus) -> business_audit::ProjectOperationsAuditStatus {
    match status {
        ProjectOperationalStatus::Planned => business_audit::ProjectOperationsAuditStatus::Planned,
        ProjectOperationalStatus::Active => business_audit::ProjectOperationsAuditStatus::Active,
        ProjectOperationalStatus::Paused => business_audit::ProjectOperationsAuditStatus::Paused,
        ProjectOperationalStatus::Completed => {
            business_audit::ProjectOperationsAuditStatus::Completed
        }
        ProjectOperationalStatus::Archived => {
            business_audit::ProjectOperationsAuditStatus::Archived
        }
    }
}

fn audit_changed_field(field: &'static str) -> business_audit::ProjectOperationsChangedField {
    match field {
        "status" => business_audit::ProjectOperationsChangedField::Status,
        "priority" => business_audit::ProjectOperationsChangedField::Priority,
        "lead_user_id" => business_audit::ProjectOperationsChangedField::LeadUserId,
        "start_date" => business_audit::ProjectOperationsChangedField::StartDate,
        "target_date" => business_audit::ProjectOperationsChangedField::TargetDate,
        "policy" => business_audit::ProjectOperationsChangedField::Policy,
        "completed_at" => business_audit::ProjectOperationsChangedField::CompletedAt,
        _ => unreachable!("domain controls project-operation changed fields"),
    }
}
