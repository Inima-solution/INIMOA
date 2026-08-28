use chrono::{Duration, Utc};
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use macro_user_id::user_id::MacroUserIdStr;
use sqlx::PgPool;
use uuid::Uuid;

use super::*;
use crate::{
    ProofMethod, ReauthenticationReceipt, ReauthenticationReceiptRepo, ReceiptPurpose,
    ReceiptScope, RequestCorrelationId,
};

fn user(value: &str) -> MacroUserIdStr<'static> {
    MacroUserIdStr::try_from(value.to_owned()).unwrap()
}

fn receipt(
    team_id: Uuid,
    principal: &str,
    purpose: ReceiptPurpose,
    issued_at: chrono::DateTime<Utc>,
) -> ReauthenticationReceipt {
    ReauthenticationReceipt::issue(
        ReceiptScope::new(team_id, user(principal), purpose),
        ProofMethod::Password,
        issued_at,
        RequestCorrelationId::try_new("request-mint").unwrap(),
    )
}

async fn consume(pool: &PgPool, receipt_id: Uuid, scope: &ReceiptScope) -> bool {
    let mut transaction = pool.begin().await.unwrap();
    let consumed =
        PgReauthenticationReceiptRepo::consume_with_tx(&mut transaction, receipt_id, scope)
            .await
            .unwrap();
    transaction.commit().await.unwrap();
    consumed
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn mint_round_trips_scope_lifetime_and_correlation(pool: PgPool) {
    let repo = PgReauthenticationReceiptRepo::new(pool.clone());
    let receipt = receipt(
        Uuid::from_u128(7),
        "macro|actor@example.com",
        ReceiptPurpose::BusinessAuditExport,
        Utc::now(),
    );
    repo.mint(&receipt).await.unwrap();

    let row: (
        Uuid,
        String,
        String,
        String,
        i64,
        String,
        Option<chrono::DateTime<Utc>>,
    ) = sqlx::query_as(
        r#"
            SELECT team_id, principal, purpose, proof_method,
                   EXTRACT(EPOCH FROM (expires_at - issued_at))::BIGINT,
                   request_id, consumed_at
            FROM reauthentication_receipts
            WHERE id = $1
            "#,
    )
    .bind(receipt.id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.0, receipt.scope.team_id);
    assert_eq!(row.1, "macro|actor@example.com");
    assert_eq!(row.2, "business_audit_export");
    assert_eq!(row.3, "password");
    assert_eq!(row.4, 300);
    assert_eq!(row.5, "request-mint");
    assert_eq!(row.6, None);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn receipt_is_one_time_and_scope_bound(pool: PgPool) {
    let repo = PgReauthenticationReceiptRepo::new(pool.clone());
    let receipt = receipt(
        Uuid::from_u128(7),
        "macro|actor@example.com",
        ReceiptPurpose::BusinessAuditExport,
        Utc::now(),
    );
    repo.mint(&receipt).await.unwrap();

    let wrong_team = ReceiptScope::new(
        Uuid::from_u128(8),
        user("macro|actor@example.com"),
        ReceiptPurpose::BusinessAuditExport,
    );
    let wrong_user = ReceiptScope::new(
        receipt.scope.team_id,
        user("macro|other@example.com"),
        ReceiptPurpose::BusinessAuditExport,
    );
    let wrong_purpose = ReceiptScope::new(
        receipt.scope.team_id,
        user("macro|actor@example.com"),
        ReceiptPurpose::CompanyRoleChange,
    );
    assert!(!consume(&pool, receipt.id, &wrong_team).await);
    assert!(!consume(&pool, receipt.id, &wrong_user).await);
    assert!(!consume(&pool, receipt.id, &wrong_purpose).await);
    assert!(consume(&pool, receipt.id, &receipt.scope).await);
    assert!(!consume(&pool, receipt.id, &receipt.scope).await);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn expired_receipt_cannot_be_consumed(pool: PgPool) {
    let repo = PgReauthenticationReceiptRepo::new(pool.clone());
    let receipt = receipt(
        Uuid::from_u128(7),
        "macro|actor@example.com",
        ReceiptPurpose::CompanyRoleChange,
        Utc::now() - Duration::minutes(6),
    );
    repo.mint(&receipt).await.unwrap();

    assert!(!consume(&pool, receipt.id, &receipt.scope).await);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn concurrent_consumers_have_exactly_one_winner(pool: PgPool) {
    let repo = PgReauthenticationReceiptRepo::new(pool.clone());
    let receipt = receipt(
        Uuid::from_u128(7),
        "macro|actor@example.com",
        ReceiptPurpose::CompanyRoleChange,
        Utc::now(),
    );
    repo.mint(&receipt).await.unwrap();

    let (first, second) = tokio::join!(
        consume(&pool, receipt.id, &receipt.scope),
        consume(&pool, receipt.id, &receipt.scope),
    );
    let wins = [first, second].into_iter().filter(|won| *won).count();
    assert_eq!(wins, 1);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn caller_transaction_rollback_restores_consumability(pool: PgPool) {
    let repo = PgReauthenticationReceiptRepo::new(pool.clone());
    let receipt = receipt(
        Uuid::from_u128(7),
        "macro|actor@example.com",
        ReceiptPurpose::CompanyRoleChange,
        Utc::now(),
    );
    repo.mint(&receipt).await.unwrap();

    let mut transaction = pool.begin().await.unwrap();
    assert!(
        PgReauthenticationReceiptRepo::consume_with_tx(
            &mut transaction,
            receipt.id,
            &receipt.scope,
        )
        .await
        .unwrap()
    );
    transaction.rollback().await.unwrap();

    assert!(consume(&pool, receipt.id, &receipt.scope).await);
}

async fn direct_insert(
    pool: &PgPool,
    principal: &str,
    purpose: &str,
    proof_method: &str,
    issued_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
    request_id: &str,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO reauthentication_receipts (
            id, team_id, principal, purpose, proof_method,
            issued_at, expires_at, request_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::from_u128(7))
    .bind(principal)
    .bind(purpose)
    .bind(proof_method)
    .bind(issued_at)
    .bind(expires_at)
    .bind(request_id)
    .execute(pool)
    .await
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn database_rejects_malformed_direct_rows(pool: PgPool) {
    let now = Utc::now();
    assert!(
        direct_insert(
            &pool,
            "bot|actor",
            "company_role_change",
            "password",
            now,
            now + Duration::minutes(5),
            "request"
        )
        .await
        .is_err()
    );
    assert!(
        direct_insert(
            &pool,
            "macro|actor@example.com",
            "other",
            "password",
            now,
            now + Duration::minutes(5),
            "request"
        )
        .await
        .is_err()
    );
    assert!(
        direct_insert(
            &pool,
            "macro|actor@example.com",
            "company_role_change",
            "passwordless_code",
            now,
            now + Duration::minutes(5),
            "request"
        )
        .await
        .is_err()
    );
    assert!(
        direct_insert(
            &pool,
            "macro|actor@example.com",
            "company_role_change",
            "password",
            now,
            now + Duration::minutes(6),
            "request"
        )
        .await
        .is_err()
    );
    assert!(
        direct_insert(
            &pool,
            "macro|actor@example.com",
            "company_role_change",
            "password",
            now,
            now + Duration::minutes(5),
            "   "
        )
        .await
        .is_err()
    );

    let oversized = "x".repeat(257);
    let result = direct_insert(
        &pool,
        "macro|actor@example.com",
        "company_role_change",
        "password",
        now,
        now + Duration::minutes(5),
        &oversized,
    )
    .await;
    assert!(result.is_err());
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn database_accepts_password_mfa_and_rejects_other_proof_methods(pool: PgPool) {
    let now = Utc::now();
    assert!(
        direct_insert(
            &pool,
            "macro|actor@example.com",
            "company_role_change",
            "password_mfa",
            now,
            now + Duration::minutes(5),
            "request"
        )
        .await
        .is_ok()
    );
    assert!(
        direct_insert(
            &pool,
            "macro|actor@example.com",
            "company_role_change",
            "trusted_device",
            now,
            now + Duration::minutes(5),
            "request"
        )
        .await
        .is_err()
    );
}
