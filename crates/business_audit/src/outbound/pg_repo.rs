//! PostgreSQL insert for business-audit events, run inside a caller-owned
//! transaction.

#[cfg(test)]
mod test;

use sqlx::postgres::PgTransaction;

use crate::domain::model::AuditEvent;

/// Inserts one event into the PostgreSQL immutable audit ledger using the
/// caller's transaction. Duplicate identifiers are errors, never absorbed.
/// Commit and rollback are owned by the caller.
pub async fn insert_with_tx(
    tx: &mut PgTransaction<'_>,
    event: &AuditEvent,
) -> Result<(), sqlx::Error> {
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
    .execute(&mut **tx)
    .await?;
    Ok(())
}
