//! Domain-owned project models.

use std::{fmt::Display, str::FromStr};

use chrono::{DateTime, NaiveDate, Utc};
use macro_user_id::user_id::MacroUserIdStr;
use model::folder::FileSystemNode;
use model::project::{BasicProject, Project};
use models_permissions::share_permission::{SharePermissionV2, UpdateSharePermissionRequestV2};
use uuid::Uuid;

#[cfg(test)]
mod test;

/// Maximum UTF-8 storage size for a project objective.
pub const PROJECT_OBJECTIVE_MAX_BYTES: usize = 2048;
/// Maximum UTF-8 storage size for a project's explicit next action.
pub const PROJECT_NEXT_ACTION_MAX_BYTES: usize = 1024;

/// The operational lifecycle state stored for a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "axum", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ProjectOperationalStatus {
    /// Work has not started.
    Planned,
    /// Work is underway.
    Active,
    /// Work is intentionally paused.
    Paused,
    /// Work is complete.
    Completed,
    /// The operational record is retained but no longer current.
    Archived,
}

/// The relative operational urgency stored for a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "axum", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ProjectPriority {
    /// Low urgency.
    Low,
    /// Normal urgency.
    Normal,
    /// High urgency.
    High,
    /// Urgent work.
    Urgent,
}

/// Error returned when a stored operational enum value is not recognized.
#[derive(Debug, thiserror::Error)]
#[error("invalid {kind} value: {value}")]
pub struct ParseProjectOperationsEnumError {
    kind: &'static str,
    value: String,
}

impl ParseProjectOperationsEnumError {
    fn new(kind: &'static str, value: &str) -> Self {
        Self {
            kind,
            value: value.to_owned(),
        }
    }
}

impl Display for ProjectOperationalStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Planned => "planned",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Archived => "archived",
        })
    }
}

impl FromStr for ProjectOperationalStatus {
    type Err = ParseProjectOperationsEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "planned" => Ok(Self::Planned),
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            "archived" => Ok(Self::Archived),
            _ => Err(ParseProjectOperationsEnumError::new(
                "ProjectOperationalStatus",
                value,
            )),
        }
    }
}

impl Display for ProjectPriority {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        })
    }
}

impl FromStr for ProjectPriority {
    type Err = ParseProjectOperationsEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            "urgent" => Ok(Self::Urgent),
            _ => Err(ParseProjectOperationsEnumError::new(
                "ProjectPriority",
                value,
            )),
        }
    }
}

/// Operational metadata attached one-to-one to a canonical project.
///
/// This model deliberately excludes project content and generic project fields.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "axum", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProjectOperations {
    /// Canonical project identifier.
    pub project_id: String,
    /// Operational lifecycle state.
    pub status: ProjectOperationalStatus,
    /// Relative operational urgency.
    pub priority: ProjectPriority,
    /// Optional operational lead.
    pub lead_user_id: Option<MacroUserIdStr<'static>>,
    /// Optional planned start date.
    pub start_date: Option<NaiveDate>,
    /// Optional planned target date.
    pub target_date: Option<NaiveDate>,
    /// Optional human-authored operational objective.
    pub objective: Option<String>,
    /// Optional human-authored next action; never inferred from task data.
    pub next_action: Option<String>,
    /// When work was completed, if recorded.
    pub completed_at: Option<DateTime<Utc>>,
    /// When the operational record was created.
    pub created_at: DateTime<Utc>,
    /// When the operational record was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optional bounded object-shaped operational policy.
    #[cfg_attr(feature = "axum", schema(value_type = Option<Object>))]
    pub policy: Option<serde_json::Value>,
}

/// The bounded, canonical overview for one project.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "axum", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProjectOverview {
    /// The canonical project row.
    #[cfg_attr(feature = "axum", schema(inline))]
    pub project: Project,
    /// The validated caller access level for this project.
    pub user_access_level: models_permissions::share_permission::access_level::AccessLevel,
    /// The canonical project operational metadata.
    pub operations: ProjectOperations,
    /// Exact counts of live, direct children only.
    pub immediate_children: ProjectOverviewImmediateChildren,
    /// Bounded aggregate progress for live direct tasks only.
    pub progress: ProjectTaskProgress,
    /// Bounded aggregate risk for live direct tasks only.
    pub risk: ProjectTaskRisk,
}

/// Exact live depth-one child counts for a project overview.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "axum", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProjectOverviewImmediateChildren {
    /// Non-deleted direct child projects.
    pub child_projects: i64,
    /// Non-deleted direct documents classified as tasks.
    pub tasks: i64,
    /// Non-deleted direct documents not classified as tasks.
    pub non_task_documents: i64,
    /// Non-deleted direct chats.
    pub chats: i64,
}

/// Repository-facing project overview data before receipt access is attached.
///
/// This is not an API model and deliberately omits caller-specific access.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectOverviewSnapshot {
    /// The canonical project row returned by the scoped repository read.
    pub project: Project,
    /// The canonical operations row returned by the scoped repository read.
    pub operations: ProjectOperations,
    /// Exact direct-child counts returned by the scoped repository read.
    pub immediate_children: ProjectOverviewImmediateChildren,
}

/// Bounded progress totals for the live direct tasks of one project.
///
/// This deliberately contains only aggregate facts: it cannot disclose task
/// identifiers, names, or individual status values.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "axum", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProjectTaskProgress {
    /// Direct live tasks whose exact singleton status is Completed.
    pub completed_tasks: i64,
    /// Direct live tasks included in progress; canceled tasks are excluded.
    pub included_tasks: i64,
    /// At least one included task had an unusable status representation.
    pub has_unavailable_statuses: bool,
}

impl ProjectTaskProgress {
    /// Builds a bounded progress result while preserving its aggregate invariant.
    pub fn new(
        completed_tasks: i64,
        included_tasks: i64,
        has_unavailable_statuses: bool,
    ) -> Result<Self, ProjectTaskProgressValidationError> {
        if completed_tasks < 0 || included_tasks < 0 || completed_tasks > included_tasks {
            return Err(ProjectTaskProgressValidationError::InvalidTotals);
        }
        if included_tasks == 0 && has_unavailable_statuses {
            return Err(ProjectTaskProgressValidationError::UnavailableWithoutIncludedTask);
        }
        Ok(Self {
            completed_tasks,
            included_tasks,
            has_unavailable_statuses,
        })
    }
}

/// Invalid aggregate task-progress totals.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectTaskProgressValidationError {
    /// Completed tasks cannot be negative or exceed included tasks.
    #[error("completed task totals must be between zero and included task totals")]
    InvalidTotals,
    /// A zero included-task result cannot have an unavailable task status.
    #[error("unavailable statuses require at least one included task")]
    UnavailableWithoutIncludedTask,
}

/// Bounded risk totals for the live direct tasks of one project.
///
/// The result intentionally contains aggregate facts only; task identities and
/// raw property values never cross the project-domain boundary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "axum", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProjectTaskRisk {
    /// Overdue direct live canonical Tasks; individual task details are redacted.
    pub overdue_tasks: i64,
    /// Blocked direct live canonical Tasks; individual task details are redacted.
    pub blocked_tasks: i64,
    /// Unassigned direct live canonical Tasks; individual task details are redacted.
    pub unassigned_tasks: i64,
    /// Whether an operational Planned or Active project target falls within seven calendar days.
    pub approaching_target: bool,
    /// Whether any aggregate risk input was unavailable without exposing its source.
    pub has_unavailable_risk_data: bool,
}

impl ProjectTaskRisk {
    /// Builds redacted aggregate risk totals for direct live canonical Tasks.
    pub fn new(
        overdue_tasks: i64,
        blocked_tasks: i64,
        unassigned_tasks: i64,
        approaching_target: bool,
        has_unavailable_risk_data: bool,
    ) -> Result<Self, ProjectTaskRiskValidationError> {
        if overdue_tasks < 0 || blocked_tasks < 0 || unassigned_tasks < 0 {
            return Err(ProjectTaskRiskValidationError::NegativeTotal);
        }
        Ok(Self {
            overdue_tasks,
            blocked_tasks,
            unassigned_tasks,
            approaching_target,
            has_unavailable_risk_data,
        })
    }
}

/// Invalid aggregate task-risk totals.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectTaskRiskValidationError {
    /// One or more aggregate direct-task counts was negative.
    #[error("task-risk totals cannot be negative")]
    NegativeTotal,
}

/// The authoritative root metadata captured with a committed project-tree purge.
#[derive(Debug, Clone, PartialEq)]
pub struct PurgedProjectTreeWithRoot {
    /// Root row locked and read in the purge transaction.
    pub root: BasicProject,
    /// Canonically removed project tree and dependent data.
    pub tree: PurgedProjectTree,
}

/// A full replacement of mutable project operational fields.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplaceProjectOperationsArgs {
    /// Requested lifecycle state.
    pub status: ProjectOperationalStatus,
    /// Requested priority.
    pub priority: ProjectPriority,
    /// Requested active-team lead, or `None` to clear it.
    pub lead_user_id: Option<MacroUserIdStr<'static>>,
    /// Requested planned start date.
    pub start_date: Option<NaiveDate>,
    /// Requested planned target date.
    pub target_date: Option<NaiveDate>,
    /// Requested human-authored objective, or `None` to clear it.
    pub objective: Option<String>,
    /// Requested human-authored next action, or `None` to clear it.
    pub next_action: Option<String>,
    /// Requested object-shaped operational policy.
    pub policy: Option<serde_json::Value>,
    /// Version observed by the caller for optimistic replacement.
    pub expected_updated_at: DateTime<Utc>,
}

/// Validation error for a project-operations replacement.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectOperationsValidationError {
    /// The requested lifecycle transition is not part of the closed state machine.
    #[error("invalid project operations transition from {from} to {to}")]
    InvalidTransition {
        /// Existing state.
        from: ProjectOperationalStatus,
        /// Requested state.
        to: ProjectOperationalStatus,
    },
    /// A target date preceded its start date.
    #[error("start_date must be on or before target_date")]
    DateOrder,
    /// Objective was present but contained only whitespace.
    #[error("objective cannot be blank")]
    ObjectiveBlank,
    /// Objective exceeded its UTF-8 storage bound.
    #[error("objective exceeds 2048 bytes")]
    ObjectiveTooLarge,
    /// Next action was present but contained only whitespace.
    #[error("next_action cannot be blank")]
    NextActionBlank,
    /// Next action exceeded its UTF-8 storage bound.
    #[error("next_action exceeds 1024 bytes")]
    NextActionTooLarge,
    /// Policy must be a JSON object when present.
    #[error("policy must be a JSON object")]
    PolicyNotObject,
    /// Compact serialized policy exceeded its durable bound.
    #[error("policy exceeds 4096 bytes")]
    PolicyTooLarge,
}

impl ReplaceProjectOperationsArgs {
    /// Checks field-local constraints before persistence.
    pub fn validate(&self) -> Result<(), ProjectOperationsValidationError> {
        if self
            .start_date
            .zip(self.target_date)
            .is_some_and(|(start, target)| start > target)
        {
            return Err(ProjectOperationsValidationError::DateOrder);
        }
        validate_narrative_field(
            self.objective.as_deref(),
            PROJECT_OBJECTIVE_MAX_BYTES,
            ProjectOperationsValidationError::ObjectiveBlank,
            ProjectOperationsValidationError::ObjectiveTooLarge,
        )?;
        validate_narrative_field(
            self.next_action.as_deref(),
            PROJECT_NEXT_ACTION_MAX_BYTES,
            ProjectOperationsValidationError::NextActionBlank,
            ProjectOperationsValidationError::NextActionTooLarge,
        )?;
        if let Some(policy) = &self.policy {
            if !policy.is_object() {
                return Err(ProjectOperationsValidationError::PolicyNotObject);
            }
            if serde_json::to_string(policy)
                .expect("serde_json::Value always serializes")
                .len()
                > 4096
            {
                return Err(ProjectOperationsValidationError::PolicyTooLarge);
            }
        }
        Ok(())
    }

    /// Applies the replacement under the lifecycle and completion-stamp rules.
    pub fn resolve(
        &self,
        current: &ProjectOperations,
        now: DateTime<Utc>,
    ) -> Result<ResolvedProjectOperationsUpdate, ProjectOperationsValidationError> {
        self.validate()?;
        if !is_valid_operations_transition(current.status, self.status) {
            return Err(ProjectOperationsValidationError::InvalidTransition {
                from: current.status,
                to: self.status,
            });
        }

        let completed_at = match (current.status, self.status) {
            (ProjectOperationalStatus::Completed, ProjectOperationalStatus::Completed)
            | (ProjectOperationalStatus::Completed, ProjectOperationalStatus::Archived) => {
                current.completed_at
            }
            (_, ProjectOperationalStatus::Completed) => Some(now),
            (_, ProjectOperationalStatus::Active | ProjectOperationalStatus::Planned) => None,
            _ => current.completed_at,
        };
        let mut changed_fields = Vec::new();
        if current.status != self.status {
            changed_fields.push("status");
        }
        if current.priority != self.priority {
            changed_fields.push("priority");
        }
        if current.lead_user_id != self.lead_user_id {
            changed_fields.push("lead_user_id");
        }
        if current.start_date != self.start_date {
            changed_fields.push("start_date");
        }
        if current.target_date != self.target_date {
            changed_fields.push("target_date");
        }
        if current.objective != self.objective {
            changed_fields.push("objective");
        }
        if current.next_action != self.next_action {
            changed_fields.push("next_action");
        }
        if current.policy != self.policy {
            changed_fields.push("policy");
        }
        if current.completed_at != completed_at {
            changed_fields.push("completed_at");
        }

        Ok(ResolvedProjectOperationsUpdate {
            status: self.status,
            priority: self.priority,
            lead_user_id: self.lead_user_id.clone(),
            start_date: self.start_date,
            target_date: self.target_date,
            objective: self.objective.clone(),
            next_action: self.next_action.clone(),
            policy: self.policy.clone(),
            completed_at,
            changed_fields,
        })
    }
}

/// A replacement resolved against the locked current record.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedProjectOperationsUpdate {
    /// Persisted lifecycle state.
    pub status: ProjectOperationalStatus,
    /// Persisted priority.
    pub priority: ProjectPriority,
    /// Persisted lead.
    pub lead_user_id: Option<MacroUserIdStr<'static>>,
    /// Persisted start date.
    pub start_date: Option<NaiveDate>,
    /// Persisted target date.
    pub target_date: Option<NaiveDate>,
    /// Persisted objective.
    pub objective: Option<String>,
    /// Persisted next action.
    pub next_action: Option<String>,
    /// Persisted object policy.
    pub policy: Option<serde_json::Value>,
    /// Server-owned completion stamp.
    pub completed_at: Option<DateTime<Utc>>,
    /// Deterministically ordered mutable fields that changed.
    pub changed_fields: Vec<&'static str>,
}

fn validate_narrative_field(
    value: Option<&str>,
    max_bytes: usize,
    blank: ProjectOperationsValidationError,
    too_large: ProjectOperationsValidationError,
) -> Result<(), ProjectOperationsValidationError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Err(blank);
    }
    if value.len() > max_bytes {
        return Err(too_large);
    }
    Ok(())
}

/// Caller-supplied portion of one operational replacement.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateProjectOperationsRequest {
    /// Canonical project identifier from the validated project receipt.
    pub project_id: String,
    /// Required request correlation identifier.
    pub request_id: String,
    /// One application-supplied time for completion, update, and audit facts.
    pub now: DateTime<Utc>,
    /// Full desired mutable state.
    pub replacement: ReplaceProjectOperationsArgs,
}

/// Verified context and desired state for one atomic operations replacement.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateProjectOperationsCommand {
    /// Canonical owner-team identifier derived from a validated company receipt.
    pub team_id: Uuid,
    /// Authenticated human actor verified against both receipts.
    pub actor_user_id: MacroUserIdStr<'static>,
    /// Caller-supplied fields, retained separately so it cannot assert authority.
    pub request: UpdateProjectOperationsRequest,
}

/// Repository result for one operational replacement.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateProjectOperationsOutcome {
    /// The row was changed and one audit fact was written in the same transaction.
    Updated(ProjectOperations),
    /// Desired state already exactly matched the locked current row.
    Unchanged(ProjectOperations),
    /// The project was missing, deleted, personal, or outside the supplied team.
    NotFound,
    /// Requested lead is not an active member of the project owner team.
    LeadNotInOwnerTeam,
    /// The supplied version was stale and desired state differed.
    Conflict,
    /// The full replacement violated a domain rule.
    Invalid(ProjectOperationsValidationError),
}

/// Returns whether a lifecycle edge is part of the closed operations state machine.
pub const fn is_valid_operations_transition(
    from: ProjectOperationalStatus,
    to: ProjectOperationalStatus,
) -> bool {
    use ProjectOperationalStatus::{Active, Archived, Completed, Paused, Planned};
    matches!(
        (from, to),
        (Planned, Planned | Active | Archived)
            | (Active, Active | Paused | Completed | Archived)
            | (Paused, Paused | Active | Completed | Archived)
            | (Completed, Completed | Active | Archived)
            | (Archived, Archived | Planned)
    )
}

/// Arguments for atomically creating a project and its access metadata.
#[derive(Debug, Clone)]
pub struct CreateProjectArgs {
    /// Project owner.
    pub user_id: String,
    /// Project name.
    pub name: String,
    /// Optional parent project.
    pub parent_id: Option<String>,
    /// Initial sharing configuration.
    pub share_permission: SharePermissionV2,
}

/// Arguments for editing a project.
#[derive(Debug, Clone)]
pub struct EditProjectArgs {
    /// Project identifier.
    pub project_id: String,
    /// Replacement name, or `None` to leave it unchanged.
    pub name: Option<String>,
    /// Whether the parent field is part of the update.
    pub update_parent: bool,
    /// Replacement parent. `None` clears it when `update_parent` is true.
    pub parent_id: Option<String>,
    /// Optional sharing changes.
    pub share_permission: Option<UpdateSharePermissionRequestV2>,
}

/// Identifiers affected by a recursive soft deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftDeleteResult {
    /// Deleted projects, including the requested root.
    pub project_ids: Vec<String>,
    /// Deleted documents.
    pub document_ids: Vec<String>,
    /// Deleted chats.
    pub chat_ids: Vec<String>,
}

/// Result of restoring a recursively deleted project tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertDeleteResult {
    /// Restored project identifiers.
    pub project_ids: Vec<String>,
    /// Restored document identifiers.
    pub document_ids: Vec<String>,
    /// Restored chat identifiers.
    pub chat_ids: Vec<String>,
}

/// Data returned after permanently purging a soft-deleted project tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurgedProjectTree {
    /// Purged project identifiers, including the requested root.
    pub project_ids: Vec<String>,
    /// Purged chat identifiers.
    pub chat_ids: Vec<String>,
    /// Purged document identifiers paired with their owner identifiers.
    pub documents: Vec<(String, String)>,
    /// Aggregated BOM part counts grouped by SHA.
    pub bom_shas: Vec<(String, i64)>,
}

/// Metadata for a project tree finalized after upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkedUploadedTree {
    /// Root project identifier.
    pub id: String,
    /// Root project name.
    pub name: String,
    /// Root project owner.
    pub user_id: MacroUserIdStr<'static>,
    /// Optional parent of the root project.
    pub parent_id: Option<String>,
    /// Finalized project identifiers, including the root and all descendants.
    pub project_ids: Vec<String>,
    /// Whether the root project's upload-pending state transitioned to finalized.
    pub upload_pending_transitioned: bool,
}

/// Arguments for creating a pending project tree for a folder upload.
#[derive(Debug)]
pub struct UploadFolderRepoArgs {
    /// Owner of every project and document in the tree.
    pub user_id: MacroUserIdStr<'static>,
    /// Initial sharing configuration for every created item.
    pub share_permission: SharePermissionV2,
    /// Root folder contents to persist.
    pub root_folder: FileSystemNode,
    /// Name of the root project.
    pub root_folder_name: String,
    /// Lambda-facing bulk-upload request identifier.
    pub upload_request_id: String,
    /// Optional parent for the root project.
    pub parent_id: Option<String>,
}

/// Result returned by project creation and editing.
pub type MutatedProject = Project;

/// Errors produced by project operations.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    /// The requested project was not found.
    #[error("project not found: {0}")]
    NotFound(String),
    /// The caller is not authorized to perform the operation.
    #[error("unauthorized")]
    Unauthorized,
    /// The caller is not authorized, with a client-facing explanation.
    #[error("{0}")]
    UnauthorizedWithMessage(String),
    /// The request is invalid.
    #[error("bad request: {0}")]
    BadRequest(String),
    /// The provided project name exceeds the maximum length.
    #[error("name too long")]
    NameTooLong {
        /// Maximum allowed name length, in grapheme clusters.
        max: usize,
    },
    /// A soft-deleted project cannot be modified.
    #[error("cannot modify deleted project")]
    CannotModifyDeleted,
    /// The requested parent would recursively nest the project.
    #[error("project is recursively nested")]
    RecursiveNesting,
    /// The request was based on a stale operational record version.
    #[error("project operations conflict")]
    Conflict,
    /// An internal operation failed.
    #[error("{0}")]
    Internal(#[from] anyhow::Error),
}
