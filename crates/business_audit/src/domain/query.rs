//! Query contract for the team-scoped immutable audit ledger.

use chrono::{DateTime, Utc};
#[cfg(feature = "outbound")]
use models_pagination::Base64Str;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Default number of ledger facts returned by a list request.
pub const DEFAULT_AUDIT_PAGE_SIZE: usize = 50;
/// Largest permitted number of ledger facts returned by a list request.
pub const MAX_AUDIT_PAGE_SIZE: usize = 100;
/// Largest number of facts a single privileged CSV export may emit.
pub const MAX_AUDIT_EXPORT_ROWS: usize = 1000;
#[cfg(feature = "outbound")]
const MAX_CURSOR_BYTES: usize = 32_000;

/// Closed retention-class filter accepted by the business audit read surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditRetentionFilter {
    /// Internal operational facts.
    Standard,
    /// Confidential people or approval facts.
    Confidential,
    /// Restricted high-risk facts.
    Restricted,
}

impl AuditRetentionFilter {
    /// SQL value for the closed filter vocabulary.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Confidential => "confidential",
            Self::Restricted => "restricted",
        }
    }
}

/// Approved, non-secret ledger fields returned to an authorized teammate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditListItem {
    /// Immutable event identity.
    pub id: Uuid,
    /// Mechanical actor principal.
    pub actor: String,
    /// Optional initiating human principal.
    pub delegated_actor: Option<String>,
    /// Stored action tag.
    pub action: String,
    /// Stored target kind.
    pub target_type: String,
    /// Canonical target identity.
    pub target_id: String,
    /// Stored outcome tag.
    pub outcome: String,
    /// Durable event time.
    pub occurred_at: DateTime<Utc>,
    /// Retention class of this fact.
    pub retention_class: AuditRetentionFilter,
}

/// Cursor-based page without counts or team-wide aggregates.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditListPage {
    /// Approved ledger facts.
    pub items: Vec<AuditListItem>,
    /// Opaque next-page position, when another page exists.
    pub next_cursor: Option<String>,
}

/// Privileged, team-scoped detail of one immutable ledger fact.
///
/// This projection is deliberately separate from `AuditListItem`: privileged
/// callers may receive the reason, request correlation, and fixed metadata,
/// while list callers never do.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditDetail {
    /// Immutable event identity.
    pub id: Uuid,
    /// Mechanical actor principal.
    pub actor: String,
    /// Optional initiating human principal.
    pub delegated_actor: Option<String>,
    /// Stored action tag.
    pub action: String,
    /// Stored target kind.
    pub target_type: String,
    /// Canonical target identity.
    pub target_id: String,
    /// Stored outcome tag.
    pub outcome: String,
    /// Durable event time.
    pub occurred_at: DateTime<Utc>,
    /// Correlation identifier for this immutable fact.
    pub request_id: String,
    /// Optional human rationale.
    pub reason: Option<String>,
    /// Fixed, action-specific safe metadata.
    pub metadata: serde_json::Value,
    /// Retention class of this fact.
    pub retention_class: AuditRetentionFilter,
}

/// Request for one privileged detail fact. The repository returns `None` for
/// both missing identifiers and identifiers owned by another team.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditDetailRequest {
    /// Team derived from the authorization receipt.
    pub team_id: Uuid,
    /// Immutable fact identifier supplied in the route path.
    pub id: Uuid,
}

/// Database-independent bounded audit export query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditExportRequest {
    /// Team derived from the authorization receipt.
    pub team_id: Uuid,
    /// Inclusive UTC start.
    pub from: DateTime<Utc>,
    /// Exclusive UTC end.
    pub until: DateTime<Utc>,
    /// Optional closed retention class.
    pub retention_class: Option<AuditRetentionFilter>,
}

/// One full immutable fact emitted by a privileged CSV export.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditExportRow {
    /// Immutable event identity.
    pub id: Uuid,
    /// Mechanical actor principal.
    pub actor: String,
    /// Optional initiating human principal.
    pub delegated_actor: Option<String>,
    /// Stored action tag.
    pub action: String,
    /// Stored target kind.
    pub target_type: String,
    /// Canonical target identity.
    pub target_id: String,
    /// Stored outcome tag.
    pub outcome: String,
    /// Durable event time.
    pub occurred_at: DateTime<Utc>,
    /// Correlation identifier for this immutable fact.
    pub request_id: String,
    /// Optional human rationale.
    pub reason: Option<String>,
    /// Fixed, action-specific safe metadata.
    pub metadata: serde_json::Value,
    /// Retention class of this fact.
    pub retention_class: AuditRetentionFilter,
}

/// Database-independent request for one team-scoped ledger page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditListRequest {
    /// Team derived from the authorization receipt.
    pub team_id: Uuid,
    /// Opaque client-supplied cursor.
    pub cursor: Option<String>,
    /// Optional closed retention class.
    pub retention_class: Option<AuditRetentionFilter>,
    /// Requested page size; it is clamped before querying.
    pub limit: Option<usize>,
}

impl AuditListRequest {
    /// Resolves the bounded page size.
    pub fn page_size(&self) -> usize {
        match self.limit {
            Some(limit) if limit < MAX_AUDIT_PAGE_SIZE => limit.max(1),
            Some(_) => MAX_AUDIT_PAGE_SIZE,
            None => DEFAULT_AUDIT_PAGE_SIZE,
        }
    }
}

/// Failure modes that remain independent from a transport implementation.
#[derive(Debug, thiserror::Error)]
pub enum AuditListError {
    /// The opaque cursor could not be decoded or did not match this request's filter.
    #[error("invalid audit cursor")]
    InvalidCursor,
    /// The immutable ledger could not be read.
    #[error("audit ledger read failed")]
    Storage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(feature = "outbound")]
pub(crate) struct AuditReadCursor {
    pub(crate) occurred_at: DateTime<Utc>,
    pub(crate) id: Uuid,
    pub(crate) retention_class: Option<AuditRetentionFilter>,
}

#[cfg(feature = "outbound")]
pub(crate) fn decode_cursor(
    cursor: Option<String>,
    retention_class: Option<AuditRetentionFilter>,
) -> Result<Option<AuditReadCursor>, AuditListError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    if cursor.len() > MAX_CURSOR_BYTES {
        return Err(AuditListError::InvalidCursor);
    }
    let cursor = Base64Str::<AuditReadCursor>::new_from_string(cursor)
        .decode_json()
        .map_err(|_| AuditListError::InvalidCursor)?;
    if cursor.retention_class != retention_class {
        return Err(AuditListError::InvalidCursor);
    }
    Ok(Some(cursor))
}

#[cfg(feature = "outbound")]
pub(crate) fn encode_cursor(
    item: &AuditListItem,
    retention_class: Option<AuditRetentionFilter>,
) -> String {
    Base64Str::encode_json(AuditReadCursor {
        occurred_at: item.occurred_at,
        id: item.id,
        retention_class,
    })
    .type_erase()
}
