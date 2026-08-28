#![deny(missing_docs)]
//! Immutable business-audit events and their insert-only storage boundary.

/// Business-audit domain types.
pub mod domain;
/// PostgreSQL adapter for the immutable ledger.
#[cfg(feature = "outbound")]
pub mod outbound;

pub use domain::model::{
    Actor, AuditAction, AuditDetailReadMetadata, AuditEvent, AuditExportedMetadata, AuditOutcome,
    AuditReason, AuditTarget, AuditTargetType, AuditValidationError, RequestCorrelationId,
    RetentionClass, RoleChangeMetadata,
};
pub use domain::query::{
    AuditDetail, AuditDetailRequest, AuditExportRequest, AuditExportRow, AuditListError,
    AuditListItem, AuditListPage, AuditListRequest, AuditRetentionFilter, DEFAULT_AUDIT_PAGE_SIZE,
    MAX_AUDIT_EXPORT_ROWS, MAX_AUDIT_PAGE_SIZE,
};
#[cfg(feature = "outbound")]
pub use outbound::{detail, export_with_tx, insert_with_tx, list};
