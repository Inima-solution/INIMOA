use chrono::{DateTime, Utc};
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use macro_user_id::user_id::MacroUserIdStr;
use models_team::BusinessRole;
use sqlx::PgPool;
use uuid::Uuid;

use super::*;
use crate::{
    Actor, AuditAction, AuditEvent, AuditOutcome, AuditReason, AuditTarget, RequestCorrelationId,
    RetentionClass, RoleChangeMetadata,
};

fn actor(value: &str) -> Actor<'static> {
    Actor::new_from_user(MacroUserIdStr::try_from(value.to_owned()).unwrap())
}

fn event(team_id: Uuid, id: u128, at: DateTime<Utc>, retention: RetentionClass) -> AuditEvent {
    let target = actor("macro|target@example.com");
    AuditEvent::new(
        team_id,
        actor("macro|actor@example.com"),
        None,
        AuditAction::RoleGranted(
            RoleChangeMetadata::new(BusinessRole::Manager, target.clone()).unwrap(),
        ),
        AuditTarget::Principal(target),
        AuditOutcome::Success,
        at,
        RequestCorrelationId::try_new(format!("read-{id}")).unwrap(),
        Some(AuditReason::try_new("ledger read test").unwrap()),
        retention,
    )
    .unwrap()
}

async fn insert(pool: &PgPool, event: &AuditEvent) {
    let mut tx = pool.begin().await.unwrap();
    crate::insert_with_tx(&mut tx, event).await.unwrap();
    tx.commit().await.unwrap();
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn list_is_team_scoped_and_keyset_stable_over_tied_timestamps(pool: PgPool) {
    let team = Uuid::from_u128(101);
    let other_team = Uuid::from_u128(102);
    let at = "2026-08-28T10:00:00Z".parse().unwrap();
    for id in [1, 2, 3] {
        insert(&pool, &event(team, id, at, RetentionClass::Standard)).await;
    }
    insert(&pool, &event(other_team, 4, at, RetentionClass::Standard)).await;

    let first = list(
        &pool,
        AuditListRequest {
            team_id: team,
            cursor: None,
            retention_class: None,
            limit: Some(2),
        },
    )
    .await
    .unwrap();
    assert_eq!(first.items.len(), 2);
    assert!(first.next_cursor.is_some());
    let second = list(
        &pool,
        AuditListRequest {
            team_id: team,
            cursor: first.next_cursor,
            retention_class: None,
            limit: Some(2),
        },
    )
    .await
    .unwrap();
    assert_eq!(second.items.len(), 1);
    let ids: std::collections::HashSet<_> = first
        .items
        .iter()
        .chain(&second.items)
        .map(|item| item.id)
        .collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(second.next_cursor, None);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn list_binds_closed_retention_filter_to_cursor_and_bounds_limit(pool: PgPool) {
    let team = Uuid::from_u128(201);
    let at = "2026-08-28T11:00:00Z".parse().unwrap();
    insert(&pool, &event(team, 1, at, RetentionClass::Standard)).await;
    insert(&pool, &event(team, 2, at, RetentionClass::Confidential)).await;
    insert(&pool, &event(team, 3, at, RetentionClass::Standard)).await;
    let filtered = list(
        &pool,
        AuditListRequest {
            team_id: team,
            cursor: None,
            retention_class: Some(AuditRetentionFilter::Standard),
            limit: Some(0),
        },
    )
    .await
    .unwrap();
    assert_eq!(filtered.items.len(), 1);
    assert_eq!(
        filtered.items[0].retention_class,
        AuditRetentionFilter::Standard
    );
    let mismatched = list(
        &pool,
        AuditListRequest {
            team_id: team,
            cursor: Some(filtered.next_cursor.unwrap()),
            retention_class: Some(AuditRetentionFilter::Confidential),
            limit: None,
        },
    )
    .await;
    assert!(matches!(mismatched, Err(AuditListError::InvalidCursor)));
    let malformed = list(
        &pool,
        AuditListRequest {
            team_id: team,
            cursor: Some("x".repeat(32_001)),
            retention_class: None,
            limit: Some(usize::MAX),
        },
    )
    .await;
    assert!(matches!(malformed, Err(AuditListError::InvalidCursor)));
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn list_empty_team_has_no_rows_or_cursor(pool: PgPool) {
    let page = list(
        &pool,
        AuditListRequest {
            team_id: Uuid::from_u128(999),
            cursor: None,
            retention_class: None,
            limit: None,
        },
    )
    .await
    .unwrap();
    assert!(page.items.is_empty());
    assert!(page.next_cursor.is_none());
}
