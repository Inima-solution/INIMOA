//! Immutable business-audit event model.

#[cfg(test)]
mod test;

use chrono::{DateTime, Utc};
use macro_user_id::user_id::MacroUserIdStr;
use models_team::BusinessRole;
use serde::Serialize;
use uuid::Uuid;

/// A principal that mechanically performed an audited action.
pub use channel_sender::ChannelSender as Actor;

const PRINCIPAL_MAX_BYTES: usize = 256;
const REQUEST_ID_MAX_BYTES: usize = 256;
const REASON_MAX_BYTES: usize = 1000;

/// Validation failure at the business-audit trust boundary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuditValidationError {
    /// A required string contained no non-whitespace characters.
    #[error("{field} must not be empty")]
    Empty {
        /// Invalid field.
        field: &'static str,
    },
    /// A string exceeded its durable storage limit.
    #[error("{field} exceeds {max_bytes} bytes")]
    TooLong {
        /// Invalid field.
        field: &'static str,
        /// Maximum encoded byte length.
        max_bytes: usize,
    },
    /// A role action did not target its grantee principal.
    #[error("role audit target must equal the grantee principal")]
    RoleTargetMismatch,
    /// A privileged audit action did not target its own team.
    #[error("privileged audit action target must equal the audit team")]
    PrivilegedAuditTeamTargetMismatch,
    /// A project-operations action did not target a canonical project.
    #[error("project operations audit target must be a project")]
    ProjectOperationsTargetMismatch,
    /// Project-operations metadata was outside the closed audit vocabulary.
    #[error("project operations audit metadata is invalid")]
    ProjectOperationsMetadataInvalid,
}

fn validate_text(
    field: &'static str,
    value: String,
    max_bytes: usize,
) -> Result<String, AuditValidationError> {
    if value.trim().is_empty() {
        return Err(AuditValidationError::Empty { field });
    }
    if value.len() > max_bytes {
        return Err(AuditValidationError::TooLong { field, max_bytes });
    }
    Ok(value)
}

/// Required request correlation identifier. It correlates events and never deduplicates them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestCorrelationId(String);

impl RequestCorrelationId {
    /// Validates a request correlation identifier.
    pub fn try_new(value: impl Into<String>) -> Result<Self, AuditValidationError> {
        validate_text("request_id", value.into(), REQUEST_ID_MAX_BYTES).map(Self)
    }
}

impl AsRef<str> for RequestCorrelationId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Optional human rationale for an audited action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditReason(String);

impl AuditReason {
    /// Validates a non-empty rationale.
    pub fn try_new(value: impl Into<String>) -> Result<Self, AuditValidationError> {
        validate_text("reason", value.into(), REASON_MAX_BYTES).map(Self)
    }
}

impl AsRef<str> for AuditReason {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Outcome of an audited attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOutcome {
    /// The operation completed.
    Success,
    /// Authorization or policy denied the operation.
    Denied,
    /// The operation was authorized but failed.
    Failed,
}

impl AuditOutcome {
    #[cfg(any(feature = "outbound", test))]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Denied => "denied",
            Self::Failed => "failed",
        }
    }
}

/// Sensitivity and retention classification of an audit event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionClass {
    /// Internal operational event.
    Standard,
    /// Confidential people or approval event.
    Confidential,
    /// Restricted payroll, secret, or high-risk event.
    Restricted,
}

impl RetentionClass {
    #[cfg(any(feature = "outbound", test))]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Confidential => "confidential",
            Self::Restricted => "restricted",
        }
    }
}

/// Closed target-kind vocabulary for the current audit foundation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditTargetType {
    /// A Macro team or company workspace.
    Team,
    /// A user, bot, or agent principal.
    Principal,
    /// A canonical project identifier.
    Project,
}

impl AuditTargetType {
    #[cfg(any(feature = "outbound", test))]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Team => "team",
            Self::Principal => "principal",
            Self::Project => "project",
        }
    }
}

/// Typed target of an audited action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditTarget {
    /// A team identified by its canonical UUID.
    Team(Uuid),
    /// A user, bot, or agent principal.
    Principal(Actor<'static>),
    /// A canonical project identifier.
    Project(String),
}

impl AuditTarget {
    #[cfg(any(feature = "outbound", test))]
    pub(crate) const fn target_type(&self) -> AuditTargetType {
        match self {
            Self::Team(_) => AuditTargetType::Team,
            Self::Principal(_) => AuditTargetType::Principal,
            Self::Project(_) => AuditTargetType::Project,
        }
    }

    #[cfg(any(feature = "outbound", test))]
    pub(crate) fn id_string(&self) -> String {
        match self {
            Self::Team(id) => id.to_string(),
            Self::Principal(principal) => principal.as_ref().to_owned(),
            Self::Project(project_id) => project_id.clone(),
        }
    }
}

/// Fixed safe metadata for a privileged immutable-fact detail read.
#[readonly::make]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditDetailReadMetadata {
    /// The immutable fact whose privileged detail was returned.
    pub audit_event_id: Uuid,
}

impl AuditDetailReadMetadata {
    /// Builds fixed metadata for a successfully returned detail fact.
    pub fn new(audit_event_id: Uuid) -> Self {
        Self { audit_event_id }
    }
}

/// Fixed safe metadata for a completed bounded audit CSV export.
#[readonly::make]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditExportedMetadata {
    /// Inclusive UTC export start.
    pub from: DateTime<Utc>,
    /// Exclusive UTC export end.
    pub until: DateTime<Utc>,
    /// Optional closed retention filter used for the export.
    pub retention_class: Option<RetentionClass>,
    /// Number of facts emitted to the CSV, bounded by the export contract.
    pub row_count: u16,
}

impl AuditExportedMetadata {
    /// Builds fixed metadata for a completed bounded export.
    pub fn new(
        from: DateTime<Utc>,
        until: DateTime<Utc>,
        retention_class: Option<RetentionClass>,
        row_count: u16,
    ) -> Self {
        Self {
            from,
            until,
            retention_class,
            row_count,
        }
    }
}

/// Fixed safe metadata for a company-role change.
#[readonly::make]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RoleChangeMetadata {
    /// Business-role bundle granted or revoked.
    pub business_role: BusinessRole,
    /// Principal affected by the role change.
    pub grantee_principal: Actor<'static>,
}

/// Fixed safe metadata for an operational project update.
///
/// It intentionally contains only lifecycle labels and field names; dates,
/// policy content, lead identities, and request payload are never retained.
#[readonly::make]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectOperationsUpdatedMetadata {
    /// Prior lifecycle state.
    from_status: ProjectOperationsAuditStatus,
    /// Resulting lifecycle state.
    to_status: ProjectOperationsAuditStatus,
    /// Deterministically ordered changed field names.
    changed_fields: Vec<ProjectOperationsChangedField>,
}

impl ProjectOperationsUpdatedMetadata {
    /// Builds bounded, allowlisted update metadata in deterministic field order.
    pub fn new(
        from_status: ProjectOperationsAuditStatus,
        to_status: ProjectOperationsAuditStatus,
        changed_fields: impl IntoIterator<Item = ProjectOperationsChangedField>,
    ) -> Result<Self, AuditValidationError> {
        let mut changed_fields: Vec<_> = changed_fields.into_iter().collect();
        changed_fields.sort();
        if changed_fields.is_empty() || changed_fields.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(AuditValidationError::ProjectOperationsMetadataInvalid);
        }
        Ok(Self {
            from_status,
            to_status,
            changed_fields,
        })
    }
}

/// Closed lifecycle labels permitted in project-operation audit metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectOperationsAuditStatus {
    /// Work has not started.
    Planned,
    /// Work is underway.
    Active,
    /// Work is paused.
    Paused,
    /// Work is complete.
    Completed,
    /// Work is archived.
    Archived,
}

/// Closed mutable-field vocabulary permitted in project-operation audit metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectOperationsChangedField {
    /// Lifecycle state changed.
    Status,
    /// Priority changed.
    Priority,
    /// Lead changed.
    LeadUserId,
    /// Start date changed.
    StartDate,
    /// Target date changed.
    TargetDate,
    /// Policy object changed.
    Policy,
    /// Completion stamp changed under the lifecycle rules.
    CompletedAt,
}

impl RoleChangeMetadata {
    /// Builds metadata after enforcing the principal storage bound.
    pub fn new(
        business_role: BusinessRole,
        grantee_principal: Actor<'static>,
    ) -> Result<Self, AuditValidationError> {
        validate_principal("grantee_principal", &grantee_principal)?;
        Ok(Self {
            business_role,
            grantee_principal,
        })
    }
}

/// Closed audited-action vocabulary for company role changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditAction {
    /// A company business role was granted.
    RoleGranted(RoleChangeMetadata),
    /// A company business role was revoked.
    RoleRevoked(RoleChangeMetadata),
    /// Privileged detail of one immutable audit fact was read.
    DetailRead(AuditDetailReadMetadata),
    /// A bounded audit CSV export was successfully emitted.
    Exported(AuditExportedMetadata),
    /// Project operational metadata was atomically updated.
    ProjectOperationsUpdated(ProjectOperationsUpdatedMetadata),
}

impl AuditAction {
    #[cfg(any(feature = "outbound", test))]
    pub(crate) const fn tag(&self) -> &'static str {
        match self {
            Self::RoleGranted(_) => "role_granted",
            Self::RoleRevoked(_) => "role_revoked",
            Self::DetailRead(_) => "audit_detail_read",
            Self::Exported(_) => "audit_exported",
            Self::ProjectOperationsUpdated(_) => "project_operations_updated",
        }
    }

    #[cfg(any(feature = "outbound", test))]
    pub(crate) fn metadata(&self) -> serde_json::Value {
        match self {
            Self::RoleGranted(metadata) | Self::RoleRevoked(metadata) => {
                serde_json::to_value(metadata)
            }
            Self::DetailRead(metadata) => serde_json::to_value(metadata),
            Self::Exported(metadata) => serde_json::to_value(metadata),
            Self::ProjectOperationsUpdated(metadata) => serde_json::to_value(metadata),
        }
        .expect("fixed audit metadata serializes")
    }

    fn validate_target(
        &self,
        target: &AuditTarget,
        team_id: Uuid,
    ) -> Result<(), AuditValidationError> {
        match (self, target) {
            (
                Self::RoleGranted(metadata) | Self::RoleRevoked(metadata),
                AuditTarget::Principal(target),
            ) if target == &metadata.grantee_principal => Ok(()),
            (Self::RoleGranted(_) | Self::RoleRevoked(_), _) => {
                Err(AuditValidationError::RoleTargetMismatch)
            }
            (Self::DetailRead(_) | Self::Exported(_), AuditTarget::Team(target))
                if target == &team_id =>
            {
                Ok(())
            }
            (Self::DetailRead(_) | Self::Exported(_), _) => {
                Err(AuditValidationError::PrivilegedAuditTeamTargetMismatch)
            }
            (Self::ProjectOperationsUpdated(_), AuditTarget::Project(project_id)) => {
                validate_text("project_id", project_id.clone(), PRINCIPAL_MAX_BYTES).map(drop)
            }
            (Self::ProjectOperationsUpdated(_), _) => {
                Err(AuditValidationError::ProjectOperationsTargetMismatch)
            }
        }
    }
}

fn validate_principal(
    field: &'static str,
    principal: &Actor<'_>,
) -> Result<(), AuditValidationError> {
    validate_text(field, principal.as_ref().to_owned(), PRINCIPAL_MAX_BYTES).map(drop)
}

/// One immutable business-audit fact.
#[readonly::make]
#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    /// Application-generated UUIDv7 identifier.
    pub id: Uuid,
    /// Company/team scope. The stored ledger intentionally has no team foreign key.
    pub team_id: Uuid,
    /// Principal that mechanically performed the action.
    pub actor: Actor<'static>,
    /// Initiating human when a bot or agent acted on their behalf.
    pub delegated_actor: Option<MacroUserIdStr<'static>>,
    /// Audited action and its fixed safe metadata.
    pub action: AuditAction,
    /// Canonical target.
    pub target: AuditTarget,
    /// Attempt outcome.
    pub outcome: AuditOutcome,
    /// Time of the action, supplied by the application.
    pub occurred_at: DateTime<Utc>,
    /// Required request correlation identifier.
    pub request_id: RequestCorrelationId,
    /// Optional human rationale, stored outside metadata.
    pub reason: Option<AuditReason>,
    /// Retention and sensitivity class.
    pub retention_class: RetentionClass,
}

impl AuditEvent {
    /// Builds an immutable event and generates its UUIDv7 identifier.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        team_id: Uuid,
        actor: Actor<'static>,
        delegated_actor: Option<MacroUserIdStr<'static>>,
        action: AuditAction,
        target: AuditTarget,
        outcome: AuditOutcome,
        occurred_at: DateTime<Utc>,
        request_id: RequestCorrelationId,
        reason: Option<AuditReason>,
        retention_class: RetentionClass,
    ) -> Result<Self, AuditValidationError> {
        validate_principal("actor", &actor)?;
        if let Some(delegated_actor) = &delegated_actor {
            validate_text(
                "delegated_actor",
                delegated_actor.as_ref().to_owned(),
                PRINCIPAL_MAX_BYTES,
            )?;
        }

        action.validate_target(&target, team_id)?;

        Ok(Self {
            id: macro_uuid::generate_uuid_v7(),
            team_id,
            actor,
            delegated_actor,
            action,
            target,
            outcome,
            occurred_at,
            request_id,
            reason,
            retention_class,
        })
    }
}
