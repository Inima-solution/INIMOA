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

#[test]
fn privileged_action_metadata_is_fixed_and_target_bound() {
    let event_id = Uuid::from_u128(9);
    let detail = AuditAction::DetailRead(AuditDetailReadMetadata::new(event_id));
    let detail_event = AuditEvent::new(
        Uuid::from_u128(1),
        actor("macro|admin@example.com"),
        None,
        detail,
        AuditTarget::Team(Uuid::from_u128(1)),
        AuditOutcome::Success,
        Utc::now(),
        RequestCorrelationId::try_new("detail-read").unwrap(),
        None,
        RetentionClass::Confidential,
    )
    .unwrap();
    assert_eq!(
        detail_event.action.metadata(),
        json!({"audit_event_id": event_id})
    );

    let exported = AuditAction::Exported(AuditExportedMetadata::new(
        "2026-08-01T00:00:00Z".parse().unwrap(),
        "2026-08-02T00:00:00Z".parse().unwrap(),
        Some(RetentionClass::Restricted),
        7,
    ));
    let export_event = AuditEvent::new(
        Uuid::from_u128(1),
        actor("macro|admin@example.com"),
        None,
        exported,
        AuditTarget::Team(Uuid::from_u128(1)),
        AuditOutcome::Success,
        Utc::now(),
        RequestCorrelationId::try_new("export").unwrap(),
        None,
        RetentionClass::Confidential,
    )
    .unwrap();
    assert_eq!(export_event.action.metadata()["row_count"], 7);
    assert!(export_event.action.metadata().get("receipt").is_none());

    let wrong_target = AuditEvent::new(
        Uuid::from_u128(1),
        actor("macro|admin@example.com"),
        None,
        AuditAction::DetailRead(AuditDetailReadMetadata::new(event_id)),
        AuditTarget::Principal(actor("macro|other@example.com")),
        AuditOutcome::Success,
        Utc::now(),
        RequestCorrelationId::try_new("wrong-target").unwrap(),
        None,
        RetentionClass::Confidential,
    );
    assert_eq!(
        wrong_target.unwrap_err(),
        AuditValidationError::PrivilegedAuditTeamTargetMismatch
    );

    let wrong_team = AuditEvent::new(
        Uuid::from_u128(1),
        actor("macro|admin@example.com"),
        None,
        AuditAction::DetailRead(AuditDetailReadMetadata::new(event_id)),
        AuditTarget::Team(Uuid::from_u128(2)),
        AuditOutcome::Success,
        Utc::now(),
        RequestCorrelationId::try_new("wrong-team").unwrap(),
        None,
        RetentionClass::Confidential,
    );
    assert_eq!(
        wrong_team.unwrap_err(),
        AuditValidationError::PrivilegedAuditTeamTargetMismatch
    );
}

#[test]
fn project_operations_audit_metadata_is_closed_canonical_and_target_bound() {
    let metadata = ProjectOperationsUpdatedMetadata::new(
        ProjectOperationsAuditStatus::Active,
        ProjectOperationsAuditStatus::Completed,
        [
            ProjectOperationsChangedField::TargetDate,
            ProjectOperationsChangedField::Status,
            ProjectOperationsChangedField::CompletedAt,
        ],
    )
    .unwrap();
    let action = AuditAction::ProjectOperationsUpdated(metadata);
    assert_eq!(action.tag(), "project_operations_updated");
    assert_eq!(
        action.metadata(),
        json!({
            "from_status": "active",
            "to_status": "completed",
            "changed_fields": ["status", "target_date", "completed_at"]
        })
    );
    assert!(action.metadata().get("lead_user_id").is_none());
    assert!(matches!(
        ProjectOperationsUpdatedMetadata::new(
            ProjectOperationsAuditStatus::Active,
            ProjectOperationsAuditStatus::Active,
            [],
        ),
        Err(AuditValidationError::ProjectOperationsMetadataInvalid)
    ));
    assert!(matches!(
        ProjectOperationsUpdatedMetadata::new(
            ProjectOperationsAuditStatus::Active,
            ProjectOperationsAuditStatus::Active,
            [
                ProjectOperationsChangedField::Priority,
                ProjectOperationsChangedField::Priority
            ],
        ),
        Err(AuditValidationError::ProjectOperationsMetadataInvalid)
    ));

    let valid = AuditEvent::new(
        Uuid::from_u128(1),
        actor("macro|admin@example.com"),
        None,
        action,
        AuditTarget::Project("project-1".to_owned()),
        AuditOutcome::Success,
        Utc::now(),
        RequestCorrelationId::try_new("operations-update").unwrap(),
        None,
        RetentionClass::Standard,
    );
    assert!(valid.is_ok());
    let mismatch = AuditEvent::new(
        Uuid::from_u128(1),
        actor("macro|admin@example.com"),
        None,
        AuditAction::ProjectOperationsUpdated(
            ProjectOperationsUpdatedMetadata::new(
                ProjectOperationsAuditStatus::Active,
                ProjectOperationsAuditStatus::Paused,
                [ProjectOperationsChangedField::Status],
            )
            .unwrap(),
        ),
        AuditTarget::Team(Uuid::from_u128(1)),
        AuditOutcome::Success,
        Utc::now(),
        RequestCorrelationId::try_new("operations-update").unwrap(),
        None,
        RetentionClass::Standard,
    );
    assert_eq!(
        mismatch.unwrap_err(),
        AuditValidationError::ProjectOperationsTargetMismatch
    );
}

#[test]
fn project_operations_project_target_is_nonempty_and_bounded() {
    let action = || {
        AuditAction::ProjectOperationsUpdated(
            ProjectOperationsUpdatedMetadata::new(
                ProjectOperationsAuditStatus::Planned,
                ProjectOperationsAuditStatus::Active,
                [ProjectOperationsChangedField::Status],
            )
            .unwrap(),
        )
    };
    for project_id in ["".to_owned(), "x".repeat(PRINCIPAL_MAX_BYTES + 1)] {
        assert!(matches!(
            AuditEvent::new(
                Uuid::from_u128(1),
                actor("macro|admin@example.com"),
                None,
                action(),
                AuditTarget::Project(project_id),
                AuditOutcome::Success,
                Utc::now(),
                RequestCorrelationId::try_new("operations-update").unwrap(),
                None,
                RetentionClass::Standard,
            ),
            Err(AuditValidationError::Empty {
                field: "project_id"
            }) | Err(AuditValidationError::TooLong {
                field: "project_id",
                ..
            })
        ));
    }
}
