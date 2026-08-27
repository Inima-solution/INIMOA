//! Receipt persistence boundary.

use super::ReauthenticationReceipt;

/// Persists newly issued reauthentication receipts.
pub trait ReauthenticationReceiptRepo: Send + Sync + 'static {
    /// Adapter error type.
    type Err: std::error::Error + Send + Sync + 'static;

    /// Inserts one newly issued receipt. Duplicate identifiers are errors.
    fn mint(
        &self,
        receipt: &ReauthenticationReceipt,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;
}
