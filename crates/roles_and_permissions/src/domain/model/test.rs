use std::str::FromStr;

use models_team::{BusinessRole, BusinessRoleSet};

use super::{PermissionId, PermissionSet, RoleId, bundle_permissions, has_business_permission};

#[test]
fn paid_subscription_roles_include_legacy_and_pricing_tiers() {
    for role in [
        RoleId::ProfessionalSubscriber,
        RoleId::TeamSubscriber,
        RoleId::Corporate,
        RoleId::SubHaiku,
        RoleId::SubSonnet,
        RoleId::SubOpus,
    ] {
        assert!(
            role.is_paid_subscription(),
            "{role} should have paid access"
        );
    }
}

#[test]
fn non_subscription_roles_do_not_grant_paid_access() {
    for role in [
        RoleId::SelfServe,
        RoleId::SuperAdmin,
        RoleId::AiSubscriber,
        RoleId::EditorUser,
    ] {
        assert!(
            !role.is_paid_subscription(),
            "{role} should not grant paid access"
        );
    }
}

const PERMISSION_STRINGS: [(PermissionId, &str); 33] = [
    (
        PermissionId::WriteStripeSubscription,
        "write:stripe_subscription",
    ),
    (
        PermissionId::ReadProfessionalFeatures,
        "read:professional_features",
    ),
    (PermissionId::WriteReleaseEmail, "write:release_email"),
    (PermissionId::WriteAdminPanel, "write:admin_panel"),
    (
        PermissionId::WriteEnterpriseSubscriptions,
        "write:enterprise_subscriptions",
    ),
    (PermissionId::WriteDiscount, "write:discount"),
    (PermissionId::WriteItPanel, "write:it_panel"),
    (PermissionId::WriteEmailTool, "write:email_tool"),
    (PermissionId::WriteAiFeatures, "write:ai_features"),
    (PermissionId::ReadDocxEditor, "read:docx_editor"),
    (PermissionId::WriteProAi, "write:proai"),
    (
        PermissionId::ReadProjectWorkScoped,
        "read:project_work:scoped",
    ),
    (PermissionId::ReadProjectWorkAll, "read:project_work:all"),
    (
        PermissionId::WriteProjectWorkStatusScoped,
        "write:project_work_status:scoped",
    ),
    (
        PermissionId::WriteProjectWorkStatusAll,
        "write:project_work_status:all",
    ),
    (PermissionId::WriteApprovalDraft, "write:approval_draft"),
    (PermissionId::WriteApprovalSubmit, "write:approval_submit"),
    (
        PermissionId::WriteApprovalDecision,
        "write:approval_decision",
    ),
    (PermissionId::ReadHrProfileOwn, "read:hr_profile:own"),
    (PermissionId::ReadHrProfileAll, "read:hr_profile:all"),
    (
        PermissionId::WriteAttendanceRequestOwn,
        "write:attendance_request:own",
    ),
    (
        PermissionId::WriteAttendanceManageTeam,
        "write:attendance_manage:team",
    ),
    (
        PermissionId::WriteAttendanceManageAll,
        "write:attendance_manage:all",
    ),
    (PermissionId::ReadPayslipOwn, "read:payslip:own"),
    (PermissionId::ReadPayslipAll, "read:payslip:all"),
    (PermissionId::WritePayroll, "write:payroll"),
    (PermissionId::ReadAuditBusiness, "read:audit:business"),
    (PermissionId::ExportAuditBusiness, "export:audit:business"),
    (PermissionId::ReadAuditHr, "read:audit:hr"),
    (PermissionId::ReadAuditPayroll, "read:audit:payroll"),
    (PermissionId::WriteCompanyRoles, "write:company_roles"),
    (PermissionId::WriteWebhooks, "write:webhooks"),
    (
        PermissionId::WriteBuildAdministration,
        "write:build_administration",
    ),
];

const INIMAOS_PERMISSIONS: [PermissionId; 22] = [
    PermissionId::ReadProjectWorkScoped,
    PermissionId::ReadProjectWorkAll,
    PermissionId::WriteProjectWorkStatusScoped,
    PermissionId::WriteProjectWorkStatusAll,
    PermissionId::WriteApprovalDraft,
    PermissionId::WriteApprovalSubmit,
    PermissionId::WriteApprovalDecision,
    PermissionId::ReadHrProfileOwn,
    PermissionId::ReadHrProfileAll,
    PermissionId::WriteAttendanceRequestOwn,
    PermissionId::WriteAttendanceManageTeam,
    PermissionId::WriteAttendanceManageAll,
    PermissionId::ReadPayslipOwn,
    PermissionId::ReadPayslipAll,
    PermissionId::WritePayroll,
    PermissionId::ReadAuditBusiness,
    PermissionId::ExportAuditBusiness,
    PermissionId::ReadAuditHr,
    PermissionId::ReadAuditPayroll,
    PermissionId::WriteCompanyRoles,
    PermissionId::WriteWebhooks,
    PermissionId::WriteBuildAdministration,
];

#[test]
fn permission_ids_round_trip_without_changing_legacy_keys() {
    for (permission, value) in PERMISSION_STRINGS {
        assert_eq!(permission.to_string(), value);
        assert_eq!(PermissionId::from_str(value).unwrap(), permission);
    }
}

#[test]
fn every_business_role_has_the_exact_permission_matrix() {
    for role in [
        BusinessRole::Member,
        BusinessRole::Manager,
        BusinessRole::Approver,
        BusinessRole::HrAdmin,
        BusinessRole::PayrollAdmin,
        BusinessRole::OrgAdmin,
        BusinessRole::Auditor,
        BusinessRole::Agent,
    ] {
        let actual = bundle_permissions(BusinessRoleSet::from_role(role));
        let expected = expected_permissions(role);
        assert_eq!(actual, expected, "incorrect permission set for {role}");

        for permission in INIMAOS_PERMISSIONS {
            assert_eq!(
                actual.contains(&permission),
                expected.contains(&permission),
                "incorrect {permission} membership for {role}",
            );
        }
    }
}

#[test]
fn role_sets_union_permissions_without_creating_hierarchy() {
    let roles = BusinessRoleSet::from_roles([BusinessRole::HrAdmin, BusinessRole::Auditor]);
    let actual = bundle_permissions(roles);
    let expected = expected_permissions(BusinessRole::HrAdmin)
        .union(&expected_permissions(BusinessRole::Auditor))
        .copied()
        .collect::<PermissionSet>();
    assert_eq!(actual, expected);

    let hr = bundle_permissions(BusinessRoleSet::from_role(BusinessRole::HrAdmin));
    assert!(hr.contains(&PermissionId::WriteAttendanceManageTeam));
    assert!(hr.contains(&PermissionId::WriteAttendanceManageAll));
    assert!(!hr.contains(&PermissionId::WriteCompanyRoles));

    let auditor = bundle_permissions(BusinessRoleSet::from_role(BusinessRole::Auditor));
    assert!(!auditor.contains(&PermissionId::WriteProjectWorkStatusScoped));
    assert!(!auditor.contains(&PermissionId::ReadHrProfileAll));
    assert!(!auditor.contains(&PermissionId::ReadPayslipAll));
}

#[test]
fn direct_business_permission_checks_match_bundle_boundaries() {
    let auditor = BusinessRoleSet::from_role(BusinessRole::Auditor);
    assert!(has_business_permission(
        BusinessRoleSet::from_role(BusinessRole::OrgAdmin),
        PermissionId::ExportAuditBusiness
    ));
    assert!(has_business_permission(
        auditor,
        PermissionId::ReadAuditBusiness
    ));
    assert!(!has_business_permission(
        auditor,
        PermissionId::ExportAuditBusiness
    ));
    assert!(has_business_permission(
        auditor,
        PermissionId::ReadProjectWorkAll
    ));
    assert!(!has_business_permission(
        auditor,
        PermissionId::WriteProjectWorkStatusScoped
    ));

    let agent = BusinessRoleSet::from_role(BusinessRole::Agent);
    assert!(has_business_permission(
        agent,
        PermissionId::WriteApprovalDraft
    ));
    assert!(!has_business_permission(
        agent,
        PermissionId::WriteApprovalSubmit
    ));

    let hr_admin = BusinessRoleSet::from_role(BusinessRole::HrAdmin);
    assert!(has_business_permission(hr_admin, PermissionId::ReadAuditHr));
    assert!(!has_business_permission(
        hr_admin,
        PermissionId::WriteCompanyRoles
    ));

    for permission in INIMAOS_PERMISSIONS {
        assert!(!has_business_permission(
            BusinessRoleSet::empty(),
            permission
        ));
    }

    let member = models_team::effective_business_roles(BusinessRoleSet::empty(), true);
    assert!(has_business_permission(
        member,
        PermissionId::WriteApprovalSubmit
    ));
}

fn expected_permissions(role: BusinessRole) -> PermissionSet {
    use PermissionId::*;

    let permissions: &[PermissionId] = match role {
        BusinessRole::Member => &[
            ReadProjectWorkScoped,
            WriteProjectWorkStatusScoped,
            WriteApprovalDraft,
            WriteApprovalSubmit,
            ReadHrProfileOwn,
            WriteAttendanceRequestOwn,
            ReadPayslipOwn,
        ],
        BusinessRole::Manager => &[
            ReadProjectWorkScoped,
            WriteProjectWorkStatusScoped,
            WriteApprovalDraft,
            WriteApprovalSubmit,
            WriteApprovalDecision,
            ReadHrProfileOwn,
            WriteAttendanceRequestOwn,
            WriteAttendanceManageTeam,
            ReadPayslipOwn,
        ],
        BusinessRole::Approver => &[
            ReadProjectWorkScoped,
            WriteProjectWorkStatusScoped,
            WriteApprovalDraft,
            WriteApprovalSubmit,
            WriteApprovalDecision,
            ReadHrProfileOwn,
            ReadPayslipOwn,
        ],
        BusinessRole::HrAdmin => &[
            ReadProjectWorkScoped,
            WriteProjectWorkStatusScoped,
            WriteApprovalDraft,
            WriteApprovalSubmit,
            WriteApprovalDecision,
            ReadHrProfileOwn,
            ReadHrProfileAll,
            WriteAttendanceRequestOwn,
            WriteAttendanceManageTeam,
            WriteAttendanceManageAll,
            ReadPayslipOwn,
            ReadAuditHr,
        ],
        BusinessRole::PayrollAdmin => &[
            ReadProjectWorkScoped,
            WriteProjectWorkStatusScoped,
            WriteApprovalDraft,
            WriteApprovalSubmit,
            WriteApprovalDecision,
            ReadHrProfileOwn,
            ReadPayslipOwn,
            ReadPayslipAll,
            WritePayroll,
            ReadAuditPayroll,
        ],
        BusinessRole::OrgAdmin => &[
            ReadProjectWorkScoped,
            ReadProjectWorkAll,
            WriteProjectWorkStatusScoped,
            WriteProjectWorkStatusAll,
            WriteApprovalDraft,
            WriteApprovalSubmit,
            WriteApprovalDecision,
            ReadHrProfileOwn,
            ReadPayslipOwn,
            ReadAuditBusiness,
            ExportAuditBusiness,
            ReadAuditHr,
            ReadAuditPayroll,
            WriteCompanyRoles,
            WriteWebhooks,
            WriteBuildAdministration,
        ],
        BusinessRole::Auditor => &[
            ReadProjectWorkScoped,
            ReadProjectWorkAll,
            ReadAuditBusiness,
            ReadAuditHr,
            ReadAuditPayroll,
        ],
        BusinessRole::Agent => &[WriteApprovalDraft],
    };

    permissions.iter().copied().collect()
}
