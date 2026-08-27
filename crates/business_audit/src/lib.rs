#![deny(missing_docs)]
//! Immutable business-audit events and their insert-only storage boundary.

/// Business-audit domain types.
pub mod domain;
/// PostgreSQL adapter for the immutable ledger.
#[cfg(feature = "outbound")]
pub mod outbound;

pub use domain::model::{
    Actor, AuditAction, AuditEvent, AuditOutcome, AuditReason, AuditTarget, AuditTargetType,
    AuditValidationError, RequestCorrelationId, RetentionClass, RoleChangeMetadata,
};
#[cfg(feature = "outbound")]
pub use outbound::insert_with_tx;
