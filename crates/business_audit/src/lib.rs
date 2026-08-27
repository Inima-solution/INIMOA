#![deny(missing_docs)]
//! Immutable business-audit events and their insert-only storage boundary.

/// Business-audit domain types and ports.
pub mod domain;
/// PostgreSQL adapter for the immutable ledger.
#[cfg(feature = "outbound")]
pub mod outbound;

pub use domain::model::{
    Actor, AuditAction, AuditEvent, AuditOutcome, AuditReason, AuditTarget, AuditTargetType,
    AuditValidationError, RequestCorrelationId, RetentionClass, RoleChangeMetadata,
};
pub use domain::ports::BusinessAuditRepo;
#[cfg(feature = "outbound")]
pub use outbound::PgBusinessAuditRepo;
