//! Storage port for business-audit events.

use super::model::AuditEvent;

/// Persists immutable business-audit facts.
pub trait BusinessAuditRepo: Send + Sync + 'static {
    /// Adapter error type.
    type Err: std::error::Error + Send + Sync + 'static;

    /// Inserts one event. Duplicate identifiers are errors, never absorbed.
    fn insert(&self, event: &AuditEvent) -> impl Future<Output = Result<(), Self::Err>> + Send;
}
