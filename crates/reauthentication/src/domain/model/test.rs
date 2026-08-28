use chrono::{TimeZone, Utc};
use macro_user_id::user_id::MacroUserIdStr;
use uuid::{Uuid, Version};

use super::*;

fn user() -> MacroUserIdStr<'static> {
    MacroUserIdStr::try_from("macro|actor@example.com".to_owned()).unwrap()
}

#[test]
fn issuance_is_random_scoped_and_expires_after_five_minutes() {
    let issued_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let scope = ReceiptScope::new(
        Uuid::from_u128(7),
        user(),
        ReceiptPurpose::CompanyRoleChange,
    );
    let first = ReauthenticationReceipt::issue(
        scope.clone(),
        ProofMethod::Password,
        issued_at,
        RequestCorrelationId::try_new("request-1").unwrap(),
    );
    let second = ReauthenticationReceipt::issue(
        scope.clone(),
        ProofMethod::Password,
        issued_at,
        RequestCorrelationId::try_new("request-2").unwrap(),
    );

    assert_eq!(first.id.get_version(), Some(Version::Random));
    assert_ne!(first.id, second.id);
    assert_eq!(first.scope, scope);
    assert_eq!((first.expires_at - first.issued_at).num_seconds(), 300);
}

#[test]
fn request_id_is_required_and_bounded() {
    assert_eq!(
        RequestCorrelationId::try_new("  "),
        Err(ReauthenticationValidationError::EmptyRequestId)
    );
    assert_eq!(
        RequestCorrelationId::try_new("x".repeat(257)),
        Err(ReauthenticationValidationError::RequestIdTooLong)
    );
}

#[test]
fn password_mfa_is_a_closed_receipt_proof_method() {
    assert_eq!(ProofMethod::PasswordMfa.as_str(), "password_mfa");
}
