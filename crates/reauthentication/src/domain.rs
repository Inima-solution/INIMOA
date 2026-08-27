//! Reauthentication receipt domain.

mod model;
mod ports;

pub use model::{
    ProofMethod, ReauthenticationReceipt, ReauthenticationValidationError, ReceiptPurpose,
    ReceiptScope, RequestCorrelationId,
};
pub use ports::ReauthenticationReceiptRepo;
