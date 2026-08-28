//! Concrete PostgreSQL service for audited company business role changes.
//!
//! `teams` owns this service because `team_business_role`, active membership,
//! and ordered team governance are one team-scoped invariant. It reuses the
//! existing MacroDB pool, the reauthentication receipt claim, and the
//! caller-transaction audit insert; it adds no repository trait.

#[cfg(test)]
mod test;

use business_audit::{
    Actor, AuditAction, AuditEvent, AuditOutcome, AuditReason, AuditTarget, AuditValidationError,
    RequestCorrelationId, RetentionClass, RoleChangeMetadata, insert_with_tx,
};
use chrono::Utc;
use macro_user_id::user_id::MacroUserIdStr;
use models_team::BusinessRole;
use reauthentication::{PgReauthenticationReceiptRepo, ReceiptPurpose, ReceiptScope};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::business_role_change::{
    BusinessRoleChangeOutcome, GrantBusinessRoleCommand, RevokeBusinessRoleCommand,
    RoleChangeDenialReason,
};
use crate::domain::model::TeamRole;

/// Failure of a company role change outside an audited policy denial.
///
/// Display stays generic on purpose: the service instruments `err`, and raw
/// database or audit details must not leak into logs. The inner error is
/// preserved as the standard error source.
#[derive(Debug, thiserror::Error)]
pub enum BusinessRoleChangeError {
    /// A database operation or an audit write failed.
    #[error("business role change database operation failed")]
    Database(#[from] sqlx::Error),
    /// An audited event failed construction validation.
    #[error("business role change audit event was invalid")]
    Audit(#[from] AuditValidationError),
}

/// Applies audited company business role changes over the existing MacroDB pool.
#[derive(Debug, Clone)]
pub struct PgBusinessRoleChangeService {
    pool: PgPool,
}

/// The attempted mutation.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Change {
    /// A `team_business_role` row is inserted.
    Grant,
    /// A `team_business_role` row is deleted.
    Revoke,
}

impl Change {
    /// The audit action for the attempted mutation.
    fn action(self, metadata: RoleChangeMetadata) -> AuditAction {
        match self {
            Self::Grant => AuditAction::RoleGranted(metadata),
            Self::Revoke => AuditAction::RoleRevoked(metadata),
        }
    }

    /// The success outcome for the attempted mutation.
    fn success(self) -> BusinessRoleChangeOutcome {
        match self {
            Self::Grant => BusinessRoleChangeOutcome::Granted,
            Self::Revoke => BusinessRoleChangeOutcome::Revoked,
        }
    }
}

impl PgBusinessRoleChangeService {
    /// Builds the service over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Grants one company business role behind a claimed reauthentication receipt.
    #[tracing::instrument(skip_all, err)]
    pub async fn grant(
        &self,
        command: &GrantBusinessRoleCommand,
    ) -> Result<BusinessRoleChangeOutcome, BusinessRoleChangeError> {
        self.change(
            Change::Grant,
            command.team_id,
            &command.actor,
            &command.target,
            command.business_role,
            command.receipt_id,
            &command.request_id,
            &command.reason,
        )
        .await
    }

    /// Revokes one company business role behind a claimed reauthentication receipt.
    #[tracing::instrument(skip_all, err)]
    pub async fn revoke(
        &self,
        command: &RevokeBusinessRoleCommand,
    ) -> Result<BusinessRoleChangeOutcome, BusinessRoleChangeError> {
        self.change(
            Change::Revoke,
            command.team_id,
            &command.actor,
            &command.target,
            command.business_role,
            command.receipt_id,
            &command.request_id,
            &command.reason,
        )
        .await
    }

    async fn change(
        &self,
        change: Change,
        team_id: Uuid,
        actor: &MacroUserIdStr<'static>,
        target: &MacroUserIdStr<'static>,
        business_role: BusinessRole,
        receipt_id: Uuid,
        request_id: &RequestCorrelationId,
        reason: &AuditReason,
    ) -> Result<BusinessRoleChangeOutcome, BusinessRoleChangeError> {
        let mut transaction = self.pool.begin().await?;

        let scope = ReceiptScope::new(team_id, actor.clone(), ReceiptPurpose::CompanyRoleChange);
        let claimed =
            PgReauthenticationReceiptRepo::consume_with_tx(&mut transaction, receipt_id, &scope)
                .await?;
        if !claimed {
            return self
                .deny(
                    change,
                    team_id,
                    actor,
                    target,
                    business_role,
                    request_id,
                    RoleChangeDenialReason::InvalidReceipt,
                    transaction,
                )
                .await;
        }

        // Lock the actor and target membership rows in lexicographic principal
        // order so concurrent inverted commands (A -> B, B -> A) cannot
        // deadlock. Self commands lock a single row.
        let mut principals = [actor.as_ref().to_owned(), target.as_ref().to_owned()];
        principals.sort();
        let distinct = usize::from(principals[0] != principals[1]) + 1;
        let mut actor_role: Option<TeamRole> = None;
        let mut target_is_member = false;
        for principal in principals.iter().take(distinct) {
            let row = sqlx::query!(
                r#"
                SELECT user_id, team_role as "team_role!: TeamRole"
                FROM team_user
                WHERE team_id = $1 AND user_id = $2
                FOR UPDATE
                "#,
                team_id,
                principal,
            )
            .fetch_optional(&mut *transaction)
            .await?;
            if let Some(row) = row {
                if principal == actor.as_ref() {
                    actor_role = Some(row.team_role);
                }
                if principal == target.as_ref() {
                    target_is_member = true;
                }
            }
        }

        // Re-check governance and membership behind the locks so a demotion or
        // target departure landing before this commit is denied truthfully.
        let denial = if actor_role < Some(TeamRole::Admin) {
            Some(RoleChangeDenialReason::InsufficientGovernance)
        } else if !target_is_member {
            Some(RoleChangeDenialReason::TargetNotMember)
        } else if business_role == BusinessRole::Member {
            Some(RoleChangeDenialReason::MemberIsDerived)
        } else if business_role == BusinessRole::Agent {
            Some(RoleChangeDenialReason::AgentRequiresAgentFlow)
        } else if change == Change::Grant && actor.as_ref() == target.as_ref() {
            Some(RoleChangeDenialReason::SelfGrant)
        } else {
            None
        };
        if let Some(denial) = denial {
            return self
                .deny(
                    change,
                    team_id,
                    actor,
                    target,
                    business_role,
                    request_id,
                    denial,
                    transaction,
                )
                .await;
        }

        let applied = match change {
            Change::Grant => {
                // Only the (team_id, principal, business_role) primary key maps
                // to a conflict; any other database error stays internal.
                let row = sqlx::query!(
                    r#"
                    INSERT INTO team_business_role (team_id, principal, business_role, granted_by)
                    VALUES ($1, $2, $3, $4)
                    ON CONFLICT (team_id, principal, business_role) DO NOTHING
                    RETURNING team_id
                    "#,
                    team_id,
                    target.as_ref(),
                    business_role as _,
                    actor.as_ref(),
                )
                .fetch_optional(&mut *transaction)
                .await?;
                row.is_some()
            }
            Change::Revoke => {
                let row = sqlx::query!(
                    r#"
                    DELETE FROM team_business_role
                    WHERE team_id = $1 AND principal = $2 AND business_role = $3
                    RETURNING team_id
                    "#,
                    team_id,
                    target.as_ref(),
                    business_role as _,
                )
                .fetch_optional(&mut *transaction)
                .await?;
                row.is_some()
            }
        };
        if !applied {
            let denial = if change == Change::Grant {
                RoleChangeDenialReason::AlreadyGranted
            } else {
                RoleChangeDenialReason::NotGranted
            };
            return self
                .deny(
                    change,
                    team_id,
                    actor,
                    target,
                    business_role,
                    request_id,
                    denial,
                    transaction,
                )
                .await;
        }

        let target_actor = Actor::new_from_user(target.clone());
        let metadata = RoleChangeMetadata::new(business_role, target_actor.clone())?;
        let event = AuditEvent::new(
            team_id,
            Actor::new_from_user(actor.clone()),
            None,
            change.action(metadata),
            AuditTarget::Principal(target_actor),
            AuditOutcome::Success,
            Utc::now(),
            request_id.clone(),
            Some(reason.clone()),
            RetentionClass::Standard,
        )?;
        insert_with_tx(&mut transaction, &event).await?;
        transaction.commit().await?;

        Ok(change.success())
    }

    /// Rolls back the main transaction, then synchronously records exactly one
    /// denied event in a fresh one-event transaction.
    async fn deny(
        &self,
        change: Change,
        team_id: Uuid,
        actor: &MacroUserIdStr<'static>,
        target: &MacroUserIdStr<'static>,
        business_role: BusinessRole,
        request_id: &RequestCorrelationId,
        denial: RoleChangeDenialReason,
        transaction: Transaction<'_, Postgres>,
    ) -> Result<BusinessRoleChangeOutcome, BusinessRoleChangeError> {
        transaction.rollback().await?;

        let denial_reason = AuditReason::try_new(denial.as_str())
            .expect("closed denial reasons are bounded machine strings");
        let target_actor = Actor::new_from_user(target.clone());
        let metadata = RoleChangeMetadata::new(business_role, target_actor.clone())?;
        let event = AuditEvent::new(
            team_id,
            Actor::new_from_user(actor.clone()),
            None,
            change.action(metadata),
            AuditTarget::Principal(target_actor),
            AuditOutcome::Denied,
            Utc::now(),
            request_id.clone(),
            Some(denial_reason),
            RetentionClass::Standard,
        )?;

        let mut denial_transaction = self.pool.begin().await?;
        insert_with_tx(&mut denial_transaction, &event).await?;
        denial_transaction.commit().await?;

        Ok(BusinessRoleChangeOutcome::Denied(denial))
    }
}
