//! Closed reauthentication receipt model.

#[cfg(test)]
mod test;

use chrono::{DateTime, Duration, Utc};
use macro_user_id::user_id::MacroUserIdStr;
use uuid::Uuid;

const REQUEST_ID_MAX_BYTES: usize = 256;
const RECEIPT_LIFETIME: Duration = Duration::minutes(5);

/// Validation failure at the receipt trust boundary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReauthenticationValidationError {
    /// The request correlation identifier was empty.
    #[error("request_id must not be empty")]
    EmptyRequestId,
    /// The request correlation identifier exceeded its storage bound.
    #[error("request_id exceeds {REQUEST_ID_MAX_BYTES} bytes")]
    RequestIdTooLong,
}

/// Required request correlation identifier. It correlates attempts and never deduplicates them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestCorrelationId(String);

impl RequestCorrelationId {
    /// Validates a request correlation identifier.
    pub fn try_new(value: impl Into<String>) -> Result<Self, ReauthenticationValidationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ReauthenticationValidationError::EmptyRequestId);
        }
        if value.len() > REQUEST_ID_MAX_BYTES {
            return Err(ReauthenticationValidationError::RequestIdTooLong);
        }
        Ok(Self(value))
    }
}

impl AsRef<str> for RequestCorrelationId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Sensitive operation authorized by a receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptPurpose {
    /// Granting or revoking a company business role.
    CompanyRoleChange,
}

impl ReceiptPurpose {
    #[cfg(any(feature = "outbound", test))]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::CompanyRoleChange => "company_role_change",
        }
    }
}

/// Interactive proof used to mint a receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofMethod {
    /// The authenticated user supplied their password to FusionAuth.
    Password,
    /// The authenticated user completed a FusionAuth MFA challenge after password verification.
    PasswordMfa,
}

impl ProofMethod {
    #[cfg(any(feature = "outbound", test))]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::PasswordMfa => "password_mfa",
        }
    }
}

/// Immutable scope used when consuming a receipt.
#[readonly::make]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptScope {
    /// Team whose sensitive operation is authorized.
    pub team_id: Uuid,
    /// Directly authenticated human principal.
    pub principal: MacroUserIdStr<'static>,
    /// Sensitive operation authorized by the receipt.
    pub purpose: ReceiptPurpose,
}

impl ReceiptScope {
    /// Builds a scope from already-validated identifiers.
    pub fn new(team_id: Uuid, principal: MacroUserIdStr<'static>, purpose: ReceiptPurpose) -> Self {
        Self {
            team_id,
            principal,
            purpose,
        }
    }
}

/// Server-side, short-lived bearer receipt. Only its random ID is exposed to clients.
#[readonly::make]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReauthenticationReceipt {
    /// Cryptographically random bearer identifier.
    pub id: Uuid,
    /// Team, human, and operation bound to this receipt.
    pub scope: ReceiptScope,
    /// Interactive proof used to mint the receipt.
    pub proof_method: ProofMethod,
    /// Time the proof was accepted.
    pub issued_at: DateTime<Utc>,
    /// Hard expiry, five minutes after issuance.
    pub expires_at: DateTime<Utc>,
    /// Correlation identifier of the mint request.
    pub request_id: RequestCorrelationId,
}

impl ReauthenticationReceipt {
    /// Issues a new opaque one-time receipt at the supplied server time.
    pub fn issue(
        scope: ReceiptScope,
        proof_method: ProofMethod,
        issued_at: DateTime<Utc>,
        request_id: RequestCorrelationId,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            scope,
            proof_method,
            issued_at,
            expires_at: issued_at + RECEIPT_LIFETIME,
            request_id,
        }
    }
}
