#[derive(
    serde::Serialize,
    serde::Deserialize,
    Eq,
    PartialEq,
    Debug,
    utoipa::ToSchema,
    Clone,
    PartialOrd,
    sqlx::Type,
    strum::EnumString,
    strum::Display,
    Copy,
    std::cmp::Ord,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "snake_case")]
#[sqlx(type_name = "\"team_role\"", rename_all = "lowercase")]
/// Ordered from least to most access top -> bottom
pub enum TeamRole {
    Member,
    Admin,
    Owner,
}

/// A company-scoped, non-hierarchical business role bundle.
#[derive(
    serde::Serialize,
    serde::Deserialize,
    Eq,
    PartialEq,
    Debug,
    utoipa::ToSchema,
    Clone,
    Copy,
    sqlx::Type,
    strum::EnumString,
    strum::Display,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[sqlx(type_name = "business_role", rename_all = "snake_case")]
pub enum BusinessRole {
    /// Baseline permissions derived from active human team membership.
    Member,
    /// Team-scoped operational management permissions.
    Manager,
    /// Rule-based approval decision permissions.
    Approver,
    /// Human-resources administration permissions.
    HrAdmin,
    /// Payroll administration permissions.
    PayrollAdmin,
    /// Company administration permissions.
    OrgAdmin,
    /// Read-only shared-work and audit permissions.
    Auditor,
    /// Explicitly scoped bot or agent permissions.
    Agent,
}

impl BusinessRole {
    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// An unordered set of company business role bundles.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BusinessRoleSet(u8);

impl BusinessRoleSet {
    /// Returns an empty role set.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns a role set containing one role.
    pub const fn from_role(role: BusinessRole) -> Self {
        Self(role.bit())
    }

    /// Returns a role set containing every supplied role.
    pub fn from_roles(roles: impl IntoIterator<Item = BusinessRole>) -> Self {
        let mut set = Self::empty();
        for role in roles {
            set.insert(role);
        }
        set
    }

    /// Adds a role to this set.
    pub fn insert(&mut self, role: BusinessRole) {
        self.0 |= role.bit();
    }

    /// Returns whether this set contains the supplied role.
    pub const fn contains(self, role: BusinessRole) -> bool {
        self.0 & role.bit() != 0
    }

    /// Returns the union of two role sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Derives baseline membership without inspecting stored role kinds.
pub fn effective_business_roles(stored: BusinessRoleSet, has_membership: bool) -> BusinessRoleSet {
    if has_membership {
        stored.union(BusinessRoleSet::from_role(BusinessRole::Member))
    } else {
        stored
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct Team {
    pub id: sqlx::types::Uuid,
    pub name: String,
    pub owner_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct TeamWithUsers {
    pub team: Team,
    pub users: Vec<TeamUser>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct TeamInvite {
    pub id: sqlx::types::Uuid,
    pub email: String,
    pub team_id: sqlx::types::Uuid,
    pub team_role: TeamRole,
    pub invited_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_sent_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct TeamUser {
    pub user_id: String,
    pub team_id: sqlx::types::Uuid,
    pub team_role: TeamRole,
}

#[derive(
    serde::Serialize,
    serde::Deserialize,
    Clone,
    Debug,
    strum::Display,
    strum::EnumString,
    utoipa::ToSchema,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TeamUpdateOperation {
    Update,
    Remove,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct TeamInviteUpdate {
    /// The team invite id to update
    pub team_invite_id: sqlx::types::Uuid,
    /// The role to assign to the invited user
    /// This is only used for `Update` operation
    pub team_role: Option<TeamRole>,
    /// The operation to perform
    /// `Update` will update the existing invitation role
    /// `Remove` will remove the invitation
    pub operation: TeamUpdateOperation,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct TeamUserUpdate {
    /// The user id to update or remove
    pub user_id: String,
    /// The new role for the user if the operation is `Update`
    pub team_role: Option<TeamRole>,
    /// The operation to perform
    /// `Update` will update the existing user role
    /// `Remove` will remove the user
    pub operation: TeamUpdateOperation,
}

/// The request body to update a team
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct PatchTeamRequest {
    /// The new name of the team
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    const ROLES: [(BusinessRole, &str); 8] = [
        (BusinessRole::Member, "member"),
        (BusinessRole::Manager, "manager"),
        (BusinessRole::Approver, "approver"),
        (BusinessRole::HrAdmin, "hr_admin"),
        (BusinessRole::PayrollAdmin, "payroll_admin"),
        (BusinessRole::OrgAdmin, "org_admin"),
        (BusinessRole::Auditor, "auditor"),
        (BusinessRole::Agent, "agent"),
    ];

    #[test]
    fn business_roles_round_trip_and_remain_independent() {
        for (role, value) in ROLES {
            assert_eq!(role.to_string(), value);
            assert_eq!(BusinessRole::from_str(value), Ok(role));

            let singleton = BusinessRoleSet::from_role(role);
            for (candidate, _) in ROLES {
                assert_eq!(singleton.contains(candidate), role == candidate);
            }
        }
    }

    #[test]
    fn role_set_supports_insert_from_roles_and_union() {
        let mut set = BusinessRoleSet::empty();
        set.insert(BusinessRole::Manager);
        assert!(set.contains(BusinessRole::Manager));

        let other = BusinessRoleSet::from_roles([BusinessRole::Approver, BusinessRole::Auditor]);
        let union = set.union(other);
        assert!(union.contains(BusinessRole::Manager));
        assert!(union.contains(BusinessRole::Approver));
        assert!(union.contains(BusinessRole::Auditor));
        assert!(!union.contains(BusinessRole::OrgAdmin));
    }

    #[test]
    fn membership_derivation_depends_only_on_membership() {
        let empty = BusinessRoleSet::empty();
        let agent = BusinessRoleSet::from_role(BusinessRole::Agent);

        let member = effective_business_roles(empty, true);
        assert!(member.contains(BusinessRole::Member));

        let agent_only = effective_business_roles(agent, false);
        assert!(agent_only.contains(BusinessRole::Agent));
        assert!(!agent_only.contains(BusinessRole::Member));

        let malformed_human = effective_business_roles(agent, true);
        assert!(malformed_human.contains(BusinessRole::Agent));
        assert!(malformed_human.contains(BusinessRole::Member));

        assert_eq!(effective_business_roles(empty, false), empty);
    }
}
