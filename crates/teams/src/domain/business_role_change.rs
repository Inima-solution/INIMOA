//! Typed commands and outcomes for audited company business role changes.
//!
//! The domain owns the closed denial vocabulary and typed results so inbound
//! adapters perform a single status mapping without HTTP types leaking here.

use business_audit::{AuditReason, RequestCorrelationId};
use macro_user_id::user_id::MacroUserIdStr;
use models_team::BusinessRole;
use uuid::Uuid;

/// Request to grant one company business role to a direct human teammate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantBusinessRoleCommand {
    /// Team in which the role is granted.
    pub team_id: Uuid,
    /// Directly authenticated human actor holding the reauthentication receipt.
    pub actor: MacroUserIdStr<'static>,
    /// Human target that receives the role.
    pub target: MacroUserIdStr<'static>,
    /// Company business role to grant.
    pub business_role: BusinessRole,
    /// One-time reauthentication receipt authorizing the change.
    pub receipt_id: Uuid,
    /// Request correlation identifier recorded on the audit event.
    pub request_id: RequestCorrelationId,
    /// Bounded human rationale recorded on the success audit event.
    pub reason: AuditReason,
}

/// Request to revoke one company business role from a direct human teammate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokeBusinessRoleCommand {
    /// Team in which the role is revoked.
    pub team_id: Uuid,
    /// Directly authenticated human actor holding the reauthentication receipt.
    pub actor: MacroUserIdStr<'static>,
    /// Human target that loses the role.
    pub target: MacroUserIdStr<'static>,
    /// Company business role to revoke.
    pub business_role: BusinessRole,
    /// One-time reauthentication receipt authorizing the change.
    pub receipt_id: Uuid,
    /// Request correlation identifier recorded on the audit event.
    pub request_id: RequestCorrelationId,
    /// Bounded human rationale recorded on the success audit event.
    pub reason: AuditReason,
}

/// Closed machine reason for an audited denial of a company role change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleChangeDenialReason {
    /// The receipt was missing, expired, consumed, or out of scope.
    InvalidReceipt,
    /// The actor does not hold ordered Admin or Owner governance.
    InsufficientGovernance,
    /// The target has no active membership row in the team.
    TargetNotMember,
    /// Member is derived from membership and cannot be stored.
    MemberIsDerived,
    /// Agent assignment requires the bot or agent flow.
    AgentRequiresAgentFlow,
    /// The actor cannot grant a role to themselves.
    SelfGrant,
    /// The target already holds the role.
    AlreadyGranted,
    /// The target does not hold the role.
    NotGranted,
}

impl RoleChangeDenialReason {
    /// Stable machine string shared with the business-audit ledger.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidReceipt => "invalid_receipt",
            Self::InsufficientGovernance => "insufficient_governance",
            Self::TargetNotMember => "target_not_member",
            Self::MemberIsDerived => "member_is_derived",
            Self::AgentRequiresAgentFlow => "agent_requires_agent_flow",
            Self::SelfGrant => "self_grant",
            Self::AlreadyGranted => "already_granted",
            Self::NotGranted => "not_granted",
        }
    }
}

/// Outcome of an audited company business role change attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessRoleChangeOutcome {
    /// The grant was applied and one success event was recorded.
    Granted,
    /// The revocation was applied and one success event was recorded.
    Revoked,
    /// The attempt was denied and one denied event was recorded.
    Denied(RoleChangeDenialReason),
}
