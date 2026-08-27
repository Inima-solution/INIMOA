#![deny(missing_docs)]
//! Short-lived, one-time proof that a human recently reauthenticated.

/// Reauthentication receipt model and persistence port.
pub mod domain;
/// PostgreSQL persistence adapter.
#[cfg(feature = "outbound")]
pub mod outbound;

pub use domain::{
    ProofMethod, ReauthenticationReceipt, ReauthenticationReceiptRepo,
    ReauthenticationValidationError, ReceiptPurpose, ReceiptScope, RequestCorrelationId,
};
#[cfg(feature = "outbound")]
pub use outbound::PgReauthenticationReceiptRepo;
