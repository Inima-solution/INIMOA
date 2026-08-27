//! PostgreSQL reauthentication receipt repository.

#[cfg(test)]
mod test;

use sqlx::{Executor, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{ReauthenticationReceipt, ReauthenticationReceiptRepo, ReceiptScope};

/// Stores and atomically consumes short-lived receipts in MacroDB.
#[derive(Debug, Clone)]
pub struct PgReauthenticationReceiptRepo {
    pool: PgPool,
}

impl PgReauthenticationReceiptRepo {
    /// Builds the repository over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Consumes a receipt inside a caller-owned transaction.
    pub async fn consume_with_tx(
        tx: &mut Transaction<'_, Postgres>,
        receipt_id: Uuid,
        scope: &ReceiptScope,
    ) -> Result<bool, sqlx::Error> {
        consume(&mut **tx, receipt_id, scope).await
    }
}

async fn consume<'e, E>(
    executor: E,
    receipt_id: Uuid,
    scope: &ReceiptScope,
) -> Result<bool, sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
    let consumed = sqlx::query_scalar!(
        r#"
        UPDATE reauthentication_receipts
        SET consumed_at = CURRENT_TIMESTAMP
        WHERE id = $1
          AND team_id = $2
          AND principal = $3
          AND purpose = $4
          AND consumed_at IS NULL
          AND expires_at > CURRENT_TIMESTAMP
        RETURNING TRUE AS "consumed!"
        "#,
        receipt_id,
        scope.team_id,
        scope.principal.as_ref(),
        scope.purpose.as_str(),
    )
    .fetch_optional(executor)
    .await?;
    Ok(consumed.unwrap_or(false))
}

impl ReauthenticationReceiptRepo for PgReauthenticationReceiptRepo {
    type Err = sqlx::Error;

    async fn mint(&self, receipt: &ReauthenticationReceipt) -> Result<(), Self::Err> {
        sqlx::query!(
            r#"
            INSERT INTO reauthentication_receipts (
                id, team_id, principal, purpose, proof_method,
                issued_at, expires_at, request_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            receipt.id,
            receipt.scope.team_id,
            receipt.scope.principal.as_ref(),
            receipt.scope.purpose.as_str(),
            receipt.proof_method.as_str(),
            receipt.issued_at,
            receipt.expires_at,
            receipt.request_id.as_ref(),
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
