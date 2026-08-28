//! Domain-owned project models.

use std::{fmt::Display, str::FromStr};

use chrono::{DateTime, NaiveDate, Utc};
use macro_user_id::user_id::MacroUserIdStr;
use model::folder::FileSystemNode;
use model::project::Project;
use models_permissions::share_permission::{SharePermissionV2, UpdateSharePermissionRequestV2};

#[cfg(test)]
mod test;

/// The operational lifecycle state stored for a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq)]
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
    /// When work was completed, if recorded.
    pub completed_at: Option<DateTime<Utc>>,
    /// When the operational record was created.
    pub created_at: DateTime<Utc>,
    /// When the operational record was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optional bounded object-shaped operational policy.
    pub policy: Option<serde_json::Value>,
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
    /// An internal operation failed.
    #[error("{0}")]
    Internal(#[from] anyhow::Error),
}
