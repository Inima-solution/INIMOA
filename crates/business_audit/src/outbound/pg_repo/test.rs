use chrono::Utc;
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use macro_user_id::user_id::MacroUserIdStr;
use models_team::BusinessRole;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::*;
use crate::{
    Actor, AuditAction, AuditEvent, AuditOutcome, AuditReason, AuditTarget, RequestCorrelationId,
    RetentionClass, RoleChangeMetadata,
};

fn user(value: &str) -> MacroUserIdStr<'static> {
    MacroUserIdStr::try_from(value.to_owned()).expect("valid test user")
}

fn actor(value: &str) -> Actor<'static> {
    Actor::new_from_user(user(value))
}

fn event(request_id: &str, delegated: Option<&str>) -> AuditEvent {
    let grantee = actor("macro|grantee@example.com");
    AuditEvent::new(
        Uuid::from_u128(11),
        actor("macro|actor@example.com"),
        delegated.map(user),
        AuditAction::RoleGranted(
            RoleChangeMetadata::new(BusinessRole::Manager, grantee.clone()).unwrap(),
        ),
        AuditTarget::Principal(grantee),
        AuditOutcome::Success,
        Utc::now(),
        RequestCorrelationId::try_new(request_id).unwrap(),
        Some(AuditReason::try_new("operational responsibility").unwrap()),
        RetentionClass::Standard,
    )
    .unwrap()
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn insert_round_trips_typed_fields_and_attribution(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    let delegated = event("request-round-trip", Some("macro|delegated@example.com"));
    insert_with_tx(&mut tx, &delegated).await.unwrap();

    let direct = event("request-direct", None);
    insert_with_tx(&mut tx, &direct).await.unwrap();
    tx.commit().await.unwrap();

    let row: (
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        Value,
        String,
    ) = sqlx::query_as(
        r#"
        SELECT actor, delegated_actor, action, target_type, target_id,
               request_id, metadata, retention_class
        FROM business_audit_events
        WHERE request_id = 'request-round-trip'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.0, "macro|actor@example.com");
    assert_eq!(row.1.as_deref(), Some("macro|delegated@example.com"));
    assert_eq!(row.2, "role_granted");
    assert_eq!(row.3, "principal");
    assert_eq!(row.4, "macro|grantee@example.com");
    assert_eq!(row.5, "request-round-trip");
    assert_eq!(
        row.6,
        json!({
            "business_role": "manager",
            "grantee_principal": "macro|grantee@example.com"
        })
    );
    assert_eq!(row.7, "standard");

    let direct_delegated: Option<String> =
        sqlx::query_scalar("SELECT delegated_actor FROM business_audit_events WHERE id = $1")
            .bind(direct.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(direct_delegated, None);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn caller_rollback_leaves_no_row(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    let event = event("request-rollback", None);
    insert_with_tx(&mut tx, &event).await.unwrap();
    tx.rollback().await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM business_audit_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn duplicate_id_is_an_error_and_keeps_one_row(pool: PgPool) {
    let event = event("request-duplicate", None);
    let mut first = pool.begin().await.unwrap();
    insert_with_tx(&mut first, &event).await.unwrap();
    first.commit().await.unwrap();

    let mut second = pool.begin().await.unwrap();
    assert!(insert_with_tx(&mut second, &event).await.is_err());
    second.rollback().await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM business_audit_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn database_rejects_update_and_delete(pool: PgPool) {
    let event = event("request-immutable", None);
    let mut tx = pool.begin().await.unwrap();
    insert_with_tx(&mut tx, &event).await.unwrap();
    tx.commit().await.unwrap();

    assert!(
        sqlx::query("UPDATE business_audit_events SET outcome = 'failed' WHERE id = $1")
            .bind(event.id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM business_audit_events WHERE id = $1")
            .bind(event.id)
            .execute(&pool)
            .await
            .is_err()
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM business_audit_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn request_id_correlates_without_deduplicating(pool: PgPool) {
    let first = event("shared-request", None);
    let second = event("shared-request", None);
    assert_ne!(first.id, second.id);

    let mut tx = pool.begin().await.unwrap();
    insert_with_tx(&mut tx, &first).await.unwrap();
    insert_with_tx(&mut tx, &second).await.unwrap();
    tx.commit().await.unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM business_audit_events WHERE request_id = $1")
            .bind("shared-request")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2);
}

async fn malformed_insert(
    pool: &PgPool,
    actor: &str,
    request_id: &str,
    reason: Option<&str>,
    metadata: Value,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO business_audit_events (
            id, team_id, actor, delegated_actor, action, target_type, target_id,
            outcome, occurred_at, request_id, reason, metadata, retention_class
        ) VALUES ($1, $2, $3, NULL, 'role_granted', 'principal',
                  'macro|grantee@example.com', 'success', $4, $5, $6, $7, 'standard')
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::from_u128(11))
    .bind(actor)
    .bind(Utc::now())
    .bind(request_id)
    .bind(reason)
    .bind(metadata)
    .execute(pool)
    .await
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn database_checks_reject_malformed_direct_rows(pool: PgPool) {
    let oversized_request_id = "x".repeat(257);
    let oversized_metadata = json!({ "value": "x".repeat(4097) });

    assert!(
        malformed_insert(&pool, "", "request", None, json!({}))
            .await
            .is_err()
    );
    assert!(
        malformed_insert(
            &pool,
            "macro|actor@example.com",
            &oversized_request_id,
            None,
            json!({})
        )
        .await
        .is_err()
    );
    assert!(
        malformed_insert(&pool, "macro|actor@example.com", "", None, json!({}))
            .await
            .is_err()
    );
    assert!(
        malformed_insert(
            &pool,
            "macro|actor@example.com",
            "request",
            Some(&"x".repeat(1001)),
            json!({})
        )
        .await
        .is_err()
    );
    assert!(
        malformed_insert(&pool, "macro|actor@example.com", "request", None, json!([]))
            .await
            .is_err()
    );
    assert!(
        malformed_insert(
            &pool,
            "macro|actor@example.com",
            "request",
            None,
            oversized_metadata
        )
        .await
        .is_err()
    );
}
