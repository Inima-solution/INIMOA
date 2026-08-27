use chrono::Utc;
use macro_user_id::user_id::MacroUserIdStr;
use models_team::BusinessRole;
use serde_json::json;

use super::*;

fn user(value: &str) -> MacroUserIdStr<'static> {
    MacroUserIdStr::try_from(value.to_owned()).expect("valid test user")
}

fn actor(value: &str) -> Actor<'static> {
    Actor::new_from_user(user(value))
}

fn metadata() -> RoleChangeMetadata {
    RoleChangeMetadata::new(BusinessRole::HrAdmin, actor("macro|grantee@example.com")).unwrap()
}

fn event(action: AuditAction) -> AuditEvent {
    AuditEvent::new(
        Uuid::from_u128(1),
        actor("macro|admin@example.com"),
        None,
        action,
        AuditTarget::Principal(actor("macro|grantee@example.com")),
        AuditOutcome::Success,
        Utc::now(),
        RequestCorrelationId::try_new("request-1").unwrap(),
        Some(AuditReason::try_new("role required for HR operations").unwrap()),
        RetentionClass::Standard,
    )
    .unwrap()
}

#[test]
fn constructor_generates_uuid_v7_and_seals_role_target() {
    let event = event(AuditAction::RoleGranted(metadata()));
    assert_eq!(event.id.get_version_num(), 7);

    let mismatch = AuditEvent::new(
        Uuid::from_u128(1),
        actor("macro|admin@example.com"),
        None,
        AuditAction::RoleGranted(metadata()),
        AuditTarget::Principal(actor("macro|other@example.com")),
        AuditOutcome::Success,
        Utc::now(),
        RequestCorrelationId::try_new("request-1").unwrap(),
        None,
        RetentionClass::Standard,
    );
    assert_eq!(
        mismatch.unwrap_err(),
        AuditValidationError::RoleTargetMismatch
    );
}

#[test]
fn role_action_tags_and_metadata_shapes_are_exact() {
    for (action, expected_tag) in [
        (AuditAction::RoleGranted(metadata()), "role_granted"),
        (AuditAction::RoleRevoked(metadata()), "role_revoked"),
    ] {
        assert_eq!(action.tag(), expected_tag);
        assert_eq!(
            action.metadata(),
            json!({
                "business_role": "hr_admin",
                "grantee_principal": "macro|grantee@example.com"
            })
        );
        assert!(action.metadata().get("reason").is_none());
        assert!(action.metadata().get("payload").is_none());
    }
}

#[test]
fn request_id_is_required_bounded_and_not_whitespace() {
    assert!(matches!(
        RequestCorrelationId::try_new("   "),
        Err(AuditValidationError::Empty {
            field: "request_id"
        })
    ));
    assert!(matches!(
        RequestCorrelationId::try_new("x".repeat(REQUEST_ID_MAX_BYTES + 1)),
        Err(AuditValidationError::TooLong {
            field: "request_id",
            max_bytes: REQUEST_ID_MAX_BYTES
        })
    ));
}

#[test]
fn optional_reason_is_nonempty_and_bounded_when_present() {
    assert!(matches!(
        AuditReason::try_new("\n\t"),
        Err(AuditValidationError::Empty { field: "reason" })
    ));
    assert!(matches!(
        AuditReason::try_new("x".repeat(REASON_MAX_BYTES + 1)),
        Err(AuditValidationError::TooLong {
            field: "reason",
            max_bytes: REASON_MAX_BYTES
        })
    ));
}

#[test]
fn closed_vocabulary_storage_tags_are_stable() {
    assert_eq!(AuditOutcome::Success.as_str(), "success");
    assert_eq!(AuditOutcome::Denied.as_str(), "denied");
    assert_eq!(AuditOutcome::Failed.as_str(), "failed");
    assert_eq!(RetentionClass::Standard.as_str(), "standard");
    assert_eq!(RetentionClass::Confidential.as_str(), "confidential");
    assert_eq!(RetentionClass::Restricted.as_str(), "restricted");
    assert_eq!(AuditTargetType::Team.as_str(), "team");
    assert_eq!(AuditTargetType::Principal.as_str(), "principal");
}
