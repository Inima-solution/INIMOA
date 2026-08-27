//! PostgreSQL business-audit repository.

#[cfg(test)]
mod test;

use sqlx::PgPool;

use crate::{BusinessAuditRepo, domain::model::AuditEvent};

/// Inserts events into the PostgreSQL immutable audit ledger.
#[derive(Debug, Clone)]
pub struct PgBusinessAuditRepo {
    pool: PgPool,
}

impl PgBusinessAuditRepo {
    /// Builds the repository over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl BusinessAuditRepo for PgBusinessAuditRepo {
    type Err = sqlx::Error;

    async fn insert(&self, event: &AuditEvent) -> Result<(), Self::Err> {
        let delegated_actor = event.delegated_actor.as_ref().map(ToString::to_string);
        let (action, metadata) = (event.action.tag(), event.action.metadata());
        let target_type = event.target.target_type().as_str();
        let target_id = event.target.id_string();
        let reason = event.reason.as_ref().map(AsRef::as_ref);

        sqlx::query!(
            r#"
            INSERT INTO business_audit_events (
                id, team_id, actor, delegated_actor, action, target_type, target_id,
                outcome, occurred_at, request_id, reason, metadata, retention_class
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8, $9, $10, $11, $12, $13
            )
            "#,
            event.id,
            event.team_id,
            event.actor.as_ref(),
            delegated_actor.as_deref(),
            action,
            target_type,
            target_id,
            event.outcome.as_str(),
            event.occurred_at,
            event.request_id.as_ref(),
            reason,
            metadata,
            event.retention_class.as_str(),
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
