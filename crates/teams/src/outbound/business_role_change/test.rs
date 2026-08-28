//! Isolated-PostgreSQL tests for the audited company role change service.

use business_audit::{AuditReason, RequestCorrelationId};
use chrono::{DateTime, Duration, Utc};
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use macro_user_id::user_id::MacroUserIdStr;
use models_team::BusinessRole;
use reauthentication::{
    ProofMethod, ReauthenticationReceipt, ReauthenticationReceiptRepo, ReceiptPurpose,
    ReceiptScope, RequestCorrelationId as MintRequestId,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::*;
use crate::domain::business_role_change::{BusinessRoleChangeOutcome, RoleChangeDenialReason};

const TEAM_1: &str = "11111111-1111-1111-1111-111111111111";
const TEAM_2: &str = "22222222-2222-2222-2222-222222222222";
const OWNER: &str = "macro|user@user.com";
const MEMBER: &str = "macro|user2@user.com";
const OUTSIDER: &str = "macro|user4@user.com";

fn team_1() -> Uuid {
    Uuid::parse_str(TEAM_1).unwrap()
}

fn team_2() -> Uuid {
    Uuid::parse_str(TEAM_2).unwrap()
}

fn user(value: &str) -> MacroUserIdStr<'static> {
    MacroUserIdStr::try_from(value.to_owned()).expect("valid test user")
}

fn service(pool: PgPool) -> PgBusinessRoleChangeService {
    PgBusinessRoleChangeService::new(pool)
}

async fn mint(
    pool: &PgPool,
    team_id: Uuid,
    principal: &str,
    issued_at: DateTime<Utc>,
) -> ReauthenticationReceipt {
    let receipt = ReauthenticationReceipt::issue(
        ReceiptScope::new(team_id, user(principal), ReceiptPurpose::CompanyRoleChange),
        ProofMethod::Password,
        issued_at,
        MintRequestId::try_new("receipt-mint").unwrap(),
    );
    PgReauthenticationReceiptRepo::new(pool.clone())
        .mint(&receipt)
        .await
        .unwrap();
    receipt
}

fn grant_command(
    team_id: Uuid,
    receipt_id: Uuid,
    request: &str,
    business_role: BusinessRole,
    actor: &str,
    target: &str,
) -> GrantBusinessRoleCommand {
    GrantBusinessRoleCommand {
        team_id,
        actor: user(actor),
        target: user(target),
        business_role,
        receipt_id,
        request_id: RequestCorrelationId::try_new(request).unwrap(),
        reason: AuditReason::try_new("operational responsibility").unwrap(),
    }
}

fn revoke_command(
    team_id: Uuid,
    receipt_id: Uuid,
    request: &str,
    business_role: BusinessRole,
    actor: &str,
    target: &str,
) -> RevokeBusinessRoleCommand {
    RevokeBusinessRoleCommand {
        team_id,
        actor: user(actor),
        target: user(target),
        business_role,
        receipt_id,
        request_id: RequestCorrelationId::try_new(request).unwrap(),
        reason: AuditReason::try_new("operational responsibility").unwrap(),
    }
}

async fn is_consumed(pool: &PgPool, receipt_id: Uuid) -> bool {
    sqlx::query_scalar(
        "SELECT consumed_at IS NOT NULL FROM reauthentication_receipts WHERE id = $1",
    )
    .bind(receipt_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Stored `(principal, business_role, granted_by)` rows for the team.
async fn role_rows(pool: &PgPool, team_id: Uuid) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT principal, business_role::text, granted_by
         FROM team_business_role
         WHERE team_id = $1
         ORDER BY principal, business_role",
    )
    .bind(team_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Audit rows `(action, outcome, reason, actor, target_id, metadata)` for one
/// request correlation.
async fn audit_rows(
    pool: &PgPool,
    request: &str,
) -> Vec<(String, String, Option<String>, String, String, Value)> {
    sqlx::query_as(
        "SELECT action, outcome, reason, actor, target_id, metadata
         FROM business_audit_events
         WHERE request_id = $1
         ORDER BY occurred_at, id",
    )
    .bind(request)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn store_role(pool: &PgPool, team_id: Uuid, principal: &str, role: &str, granted_by: &str) {
    sqlx::query(
        "INSERT INTO team_business_role (team_id, principal, business_role, granted_by)
         VALUES ($1, $2, $3::business_role, $4)",
    )
    .bind(team_id)
    .bind(principal)
    .bind(role)
    .bind(granted_by)
    .execute(pool)
    .await
    .unwrap();
}

async fn lock_member_row(tx: &mut sqlx::postgres::PgTransaction<'_>, user_id: &str) {
    sqlx::query("SELECT user_id FROM team_user WHERE team_id = $1 AND user_id = $2 FOR UPDATE")
        .bind(team_1())
        .bind(user_id)
        .fetch_one(&mut **tx)
        .await
        .unwrap();
}

/// Waits until some other backend on this test database blocks on a row lock,
/// so a race can commit its mutation while the command is mid-transaction.
async fn wait_for_lock_waiter(pool: &PgPool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pg_stat_activity
             WHERE pid <> pg_backend_pid()
               AND datname = current_database()
               AND wait_event_type = 'Lock'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting > 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("command never reached the membership row lock");
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn grant_is_atomic_over_receipt_role_and_audit(pool: PgPool) {
    let svc = service(pool.clone());
    let receipt = mint(&pool, team_1(), OWNER, Utc::now()).await;
    let command = grant_command(
        team_1(),
        receipt.id,
        "request-grant",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );

    assert_eq!(
        svc.grant(&command).await.unwrap(),
        BusinessRoleChangeOutcome::Granted
    );

    assert!(is_consumed(&pool, receipt.id).await);
    assert_eq!(
        role_rows(&pool, team_1()).await,
        vec![(MEMBER.into(), "manager".into(), OWNER.into())]
    );

    let rows = audit_rows(&pool, "request-grant").await;
    assert_eq!(rows.len(), 1);
    let (action, outcome, reason, actor, target_id, metadata) = &rows[0];
    assert_eq!(action, "role_granted");
    assert_eq!(outcome, "success");
    assert_eq!(reason.as_deref(), Some("operational responsibility"));
    assert_eq!(actor, OWNER);
    assert_eq!(target_id, MEMBER);
    assert_eq!(
        metadata,
        &json!({ "business_role": "manager", "grantee_principal": MEMBER })
    );

    let receipt_leak: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM business_audit_events
            WHERE request_id = 'request-grant'
              AND (reason = $1 OR metadata::text LIKE $2)
        )",
    )
    .bind(receipt.id.to_string())
    .bind(format!("%{}%", receipt.id))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!receipt_leak);
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn revoke_is_atomic_including_self_revoke(pool: PgPool) {
    let svc = service(pool.clone());

    store_role(&pool, team_1(), MEMBER, "manager", OWNER).await;
    let receipt = mint(&pool, team_1(), OWNER, Utc::now()).await;
    let command = revoke_command(
        team_1(),
        receipt.id,
        "request-revoke",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.revoke(&command).await.unwrap(),
        BusinessRoleChangeOutcome::Revoked
    );
    assert!(is_consumed(&pool, receipt.id).await);
    assert!(role_rows(&pool, team_1()).await.is_empty());

    let rows = audit_rows(&pool, "request-revoke").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "role_revoked");
    assert_eq!(rows[0].1, "success");
    assert_eq!(rows[0].4, MEMBER);

    // Self-revoke is allowed and locks a single membership row.
    store_role(&pool, team_1(), OWNER, "auditor", OWNER).await;
    let self_receipt = mint(&pool, team_1(), OWNER, Utc::now()).await;
    let self_command = revoke_command(
        team_1(),
        self_receipt.id,
        "request-self-revoke",
        BusinessRole::Auditor,
        OWNER,
        OWNER,
    );
    assert_eq!(
        svc.revoke(&self_command).await.unwrap(),
        BusinessRoleChangeOutcome::Revoked
    );
    assert!(is_consumed(&pool, self_receipt.id).await);
    assert!(role_rows(&pool, team_1()).await.is_empty());
    assert_eq!(audit_rows(&pool, "request-self-revoke").await.len(), 1);
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn audit_insertion_failure_rolls_back_receipt_and_state(pool: PgPool) {
    sqlx::query(
        r#"
        CREATE FUNCTION fail_member_audit() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN
            IF NEW.target_id = 'macro|user2@user.com' THEN
                RAISE EXCEPTION 'forced audit failure';
            END IF;
            RETURN NEW;
        END $$;
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_member_audit
         BEFORE INSERT ON business_audit_events
         FOR EACH ROW EXECUTE FUNCTION fail_member_audit();",
    )
    .execute(&pool)
    .await
    .unwrap();

    let svc = service(pool.clone());

    // Success path whose audit insert fails: receipt and role state unchanged.
    let grant_receipt = mint(&pool, team_1(), OWNER, Utc::now()).await;
    let grant = grant_command(
        team_1(),
        grant_receipt.id,
        "request-audit-fail",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert!(matches!(
        svc.grant(&grant).await,
        Err(BusinessRoleChangeError::Database(_))
    ));
    assert!(!is_consumed(&pool, grant_receipt.id).await);
    assert!(role_rows(&pool, team_1()).await.is_empty());
    assert!(audit_rows(&pool, "request-audit-fail").await.is_empty());

    // Denial path whose audit insert fails: the denial is not reported as
    // normally completed.
    let denial_receipt = mint(&pool, team_1(), OWNER, Utc::now()).await;
    let denial = grant_command(
        team_1(),
        denial_receipt.id,
        "request-denial-audit-fail",
        BusinessRole::Member,
        OWNER,
        MEMBER,
    );
    assert!(matches!(
        svc.grant(&denial).await,
        Err(BusinessRoleChangeError::Database(_))
    ));
    assert!(!is_consumed(&pool, denial_receipt.id).await);
    assert!(
        audit_rows(&pool, "request-denial-audit-fail")
            .await
            .is_empty()
    );

    sqlx::query("DROP TRIGGER fail_member_audit ON business_audit_events;")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION fail_member_audit();")
        .execute(&pool)
        .await
        .unwrap();
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn all_eight_denials_are_recorded_truthfully(pool: PgPool) {
    let svc = service(pool.clone());
    let now = Utc::now();

    // 1. invalid_receipt: the receipt was never issued.
    let missing = grant_command(
        team_1(),
        Uuid::new_v4(),
        "denial-1",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.grant(&missing).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::InvalidReceipt)
    );

    // 2. insufficient_governance: the actor is a plain member.
    let receipt = mint(&pool, team_1(), MEMBER, now).await;
    let under_governed = grant_command(
        team_1(),
        receipt.id,
        "denial-2",
        BusinessRole::Manager,
        MEMBER,
        OWNER,
    );
    assert_eq!(
        svc.grant(&under_governed).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::InsufficientGovernance)
    );
    assert!(!is_consumed(&pool, receipt.id).await);

    // 3. target_not_member: the target belongs to a different team.
    let receipt = mint(&pool, team_1(), OWNER, now).await;
    let not_member = grant_command(
        team_1(),
        receipt.id,
        "denial-3",
        BusinessRole::Manager,
        OWNER,
        OUTSIDER,
    );
    assert_eq!(
        svc.grant(&not_member).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::TargetNotMember)
    );

    // 4. member_is_derived: Member is derived from membership.
    let receipt = mint(&pool, team_1(), OWNER, now).await;
    let derived = grant_command(
        team_1(),
        receipt.id,
        "denial-4",
        BusinessRole::Member,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.grant(&derived).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::MemberIsDerived)
    );

    // 5. agent_requires_agent_flow: Agent assignment is not a human flow.
    let receipt = mint(&pool, team_1(), OWNER, now).await;
    let agent = grant_command(
        team_1(),
        receipt.id,
        "denial-5",
        BusinessRole::Agent,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.grant(&agent).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::AgentRequiresAgentFlow)
    );

    // 6. self_grant: the actor cannot grant to themselves.
    let receipt = mint(&pool, team_1(), OWNER, now).await;
    let self_grant = grant_command(
        team_1(),
        receipt.id,
        "denial-6",
        BusinessRole::Manager,
        OWNER,
        OWNER,
    );
    assert_eq!(
        svc.grant(&self_grant).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::SelfGrant)
    );

    // 7. already_granted: the role row already exists.
    store_role(&pool, team_1(), MEMBER, "manager", OWNER).await;
    let receipt = mint(&pool, team_1(), OWNER, now).await;
    let duplicate = grant_command(
        team_1(),
        receipt.id,
        "denial-7",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.grant(&duplicate).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::AlreadyGranted)
    );

    // 8. not_granted: there is no stored row to revoke.
    let receipt = mint(&pool, team_1(), OWNER, now).await;
    let absent = revoke_command(
        team_1(),
        receipt.id,
        "denial-8",
        BusinessRole::Approver,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.revoke(&absent).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::NotGranted)
    );

    // Every denial is exactly one denied event carrying the machine reason,
    // and claimed receipts are restored by the rollback.
    let expected: [(&str, &str); 8] = [
        ("invalid_receipt", "role_granted"),
        ("insufficient_governance", "role_granted"),
        ("target_not_member", "role_granted"),
        ("member_is_derived", "role_granted"),
        ("agent_requires_agent_flow", "role_granted"),
        ("self_grant", "role_granted"),
        ("already_granted", "role_granted"),
        ("not_granted", "role_revoked"),
    ];
    for (index, (reason, action)) in expected.iter().enumerate() {
        let rows = audit_rows(&pool, &format!("denial-{}", index + 1)).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, *action);
        assert_eq!(rows[0].1, "denied");
        assert_eq!(rows[0].2.as_deref(), Some(*reason));
    }

    // Only the pre-stored row remains and no receipt id reached the ledger.
    assert_eq!(
        role_rows(&pool, team_1()).await,
        vec![(MEMBER.into(), "manager".into(), OWNER.into())]
    );
    let leaked: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM reauthentication_receipts r
         WHERE EXISTS (
             SELECT 1 FROM business_audit_events e
             WHERE e.request_id LIKE 'denial-%'
               AND (e.reason = r.id::text OR e.metadata::text LIKE '%' || r.id::text || '%')
         )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!leaked);
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn conflict_denial_keeps_receipt_consumable(pool: PgPool) {
    let svc = service(pool.clone());

    let first = mint(&pool, team_1(), OWNER, Utc::now()).await;
    let grant = grant_command(
        team_1(),
        first.id,
        "conflict-1",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.grant(&grant).await.unwrap(),
        BusinessRoleChangeOutcome::Granted
    );

    // The conflicting grant is denied but leaves its receipt consumable.
    let second = mint(&pool, team_1(), OWNER, Utc::now()).await;
    let conflict = grant_command(
        team_1(),
        second.id,
        "conflict-2",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.grant(&conflict).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::AlreadyGranted)
    );
    assert!(!is_consumed(&pool, second.id).await);

    // The same receipt authorizes a later valid command.
    let revoke = revoke_command(
        team_1(),
        second.id,
        "conflict-3",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.revoke(&revoke).await.unwrap(),
        BusinessRoleChangeOutcome::Revoked
    );
    assert!(is_consumed(&pool, second.id).await);
    assert!(role_rows(&pool, team_1()).await.is_empty());
    assert_eq!(audit_rows(&pool, "conflict-1").await.len(), 1);
    assert_eq!(audit_rows(&pool, "conflict-2").await.len(), 1);
    assert_eq!(audit_rows(&pool, "conflict-3").await.len(), 1);
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn concurrent_same_receipt_commands_have_one_winner(pool: PgPool) {
    let svc = service(pool.clone());
    let receipt = mint(&pool, team_1(), OWNER, Utc::now()).await;

    let first = grant_command(
        team_1(),
        receipt.id,
        "race-receipt-1",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    let second = grant_command(
        team_1(),
        receipt.id,
        "race-receipt-2",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );

    let (first, second) = tokio::join!(svc.grant(&first), svc.grant(&second));
    let outcomes = [first.unwrap(), second.unwrap()];
    let granted = outcomes
        .iter()
        .filter(|outcome| **outcome == BusinessRoleChangeOutcome::Granted)
        .count();
    let invalid = outcomes
        .iter()
        .filter(|outcome| {
            **outcome == BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::InvalidReceipt)
        })
        .count();
    assert_eq!(granted, 1);
    assert_eq!(invalid, 1);

    assert!(is_consumed(&pool, receipt.id).await);
    assert_eq!(
        role_rows(&pool, team_1()).await,
        vec![(MEMBER.into(), "manager".into(), OWNER.into())]
    );
    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM business_audit_events WHERE request_id LIKE 'race-receipt-%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(events, 2);
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn concurrent_same_role_commands_restore_the_losing_receipt(pool: PgPool) {
    let svc = service(pool.clone());
    let first_receipt = mint(&pool, team_1(), OWNER, Utc::now()).await;
    let second_receipt = mint(&pool, team_1(), OWNER, Utc::now()).await;

    let first = grant_command(
        team_1(),
        first_receipt.id,
        "race-role-1",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    let second = grant_command(
        team_1(),
        second_receipt.id,
        "race-role-2",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );

    let (first, second) = tokio::join!(svc.grant(&first), svc.grant(&second));
    let (first, second) = (first.unwrap(), second.unwrap());
    let granted = [first, second]
        .iter()
        .filter(|outcome| **outcome == BusinessRoleChangeOutcome::Granted)
        .count();
    let conflicted = [first, second]
        .iter()
        .filter(|outcome| {
            **outcome == BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::AlreadyGranted)
        })
        .count();
    assert_eq!(granted, 1);
    assert_eq!(conflicted, 1);

    // The losing receipt is restored and authorizes a later valid command.
    let loser = if first == BusinessRoleChangeOutcome::Granted {
        second_receipt.id
    } else {
        first_receipt.id
    };
    let revoke = revoke_command(
        team_1(),
        loser,
        "race-role-3",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.revoke(&revoke).await.unwrap(),
        BusinessRoleChangeOutcome::Revoked
    );
    assert!(is_consumed(&pool, loser).await);
    assert!(role_rows(&pool, team_1()).await.is_empty());
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn cross_actor_inverted_orders_do_not_deadlock(pool: PgPool) {
    sqlx::query("UPDATE team_user SET team_role = 'admin' WHERE team_id = $1 AND user_id = $2")
        .bind(team_1())
        .bind(MEMBER)
        .execute(&pool)
        .await
        .unwrap();

    let svc = service(pool.clone());
    let owner_receipt = mint(&pool, team_1(), OWNER, Utc::now()).await;
    let member_receipt = mint(&pool, team_1(), MEMBER, Utc::now()).await;

    let a_to_b = grant_command(
        team_1(),
        owner_receipt.id,
        "inverted-1",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    let b_to_a = grant_command(
        team_1(),
        member_receipt.id,
        "inverted-2",
        BusinessRole::Approver,
        MEMBER,
        OWNER,
    );

    let (a_to_b, b_to_a) = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let (first, second) = tokio::join!(svc.grant(&a_to_b), svc.grant(&b_to_a));
        (first.unwrap(), second.unwrap())
    })
    .await
    .expect("inverted lock orders must not deadlock");

    assert_eq!(a_to_b, BusinessRoleChangeOutcome::Granted);
    assert_eq!(b_to_a, BusinessRoleChangeOutcome::Granted);
    assert!(is_consumed(&pool, owner_receipt.id).await);
    assert!(is_consumed(&pool, member_receipt.id).await);
    assert_eq!(role_rows(&pool, team_1()).await.len(), 2);
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn demotion_landing_before_commit_denies_governance(pool: PgPool) {
    sqlx::query("UPDATE team_user SET team_role = 'admin' WHERE team_id = $1 AND user_id = $2")
        .bind(team_1())
        .bind(MEMBER)
        .execute(&pool)
        .await
        .unwrap();

    let svc = service(pool.clone());
    let receipt = mint(&pool, team_1(), MEMBER, Utc::now()).await;
    let command = grant_command(
        team_1(),
        receipt.id,
        "demotion-race",
        BusinessRole::Manager,
        MEMBER,
        OWNER,
    );

    // Hold the membership lock, let the command block on it, then demote.
    let mut demoter = pool.begin().await.unwrap();
    lock_member_row(&mut demoter, MEMBER).await;
    let pending = tokio::spawn({
        let svc = svc.clone();
        async move { svc.grant(&command).await }
    });
    wait_for_lock_waiter(&pool).await;
    sqlx::query("UPDATE team_user SET team_role = 'member' WHERE team_id = $1 AND user_id = $2")
        .bind(team_1())
        .bind(MEMBER)
        .execute(&mut *demoter)
        .await
        .unwrap();
    demoter.commit().await.unwrap();

    let outcome = pending.await.unwrap().unwrap();
    assert_eq!(
        outcome,
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::InsufficientGovernance)
    );
    assert!(!is_consumed(&pool, receipt.id).await);
    let rows = audit_rows(&pool, "demotion-race").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "role_granted");
    assert_eq!(rows[0].1, "denied");
    assert_eq!(rows[0].2.as_deref(), Some("insufficient_governance"));
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn target_removal_landing_before_commit_denies_membership(pool: PgPool) {
    let svc = service(pool.clone());
    let receipt = mint(&pool, team_1(), OWNER, Utc::now()).await;
    let command = grant_command(
        team_1(),
        receipt.id,
        "removal-race",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );

    // Hold the membership lock, let the command block on it, then remove.
    let mut remover = pool.begin().await.unwrap();
    lock_member_row(&mut remover, MEMBER).await;
    let pending = tokio::spawn({
        let svc = svc.clone();
        async move { svc.grant(&command).await }
    });
    wait_for_lock_waiter(&pool).await;
    sqlx::query("DELETE FROM team_user WHERE team_id = $1 AND user_id = $2")
        .bind(team_1())
        .bind(MEMBER)
        .execute(&mut *remover)
        .await
        .unwrap();
    remover.commit().await.unwrap();

    let outcome = pending.await.unwrap().unwrap();
    assert_eq!(
        outcome,
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::TargetNotMember)
    );
    assert!(!is_consumed(&pool, receipt.id).await);
    let rows = audit_rows(&pool, "removal-race").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "role_granted");
    assert_eq!(rows[0].1, "denied");
    assert_eq!(rows[0].2.as_deref(), Some("target_not_member"));
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("teams"))
)]
async fn out_of_scope_and_unusable_receipts_are_denied(pool: PgPool) {
    let svc = service(pool.clone());
    let now = Utc::now();

    // Cross-team: the receipt was issued for a different team.
    let cross_team = mint(&pool, team_2(), OWNER, now).await;
    let command = grant_command(
        team_1(),
        cross_team.id,
        "scope-cross-team",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.grant(&command).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::InvalidReceipt)
    );
    assert!(!is_consumed(&pool, cross_team.id).await);

    // Cross-principal: the receipt was issued to a different human.
    let cross_principal = mint(&pool, team_1(), MEMBER, now).await;
    let command = grant_command(
        team_1(),
        cross_principal.id,
        "scope-cross-principal",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.grant(&command).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::InvalidReceipt)
    );
    assert!(!is_consumed(&pool, cross_principal.id).await);

    // Expired past the five-minute lifetime.
    let expired = mint(&pool, team_1(), OWNER, now - Duration::minutes(6)).await;
    let command = grant_command(
        team_1(),
        expired.id,
        "scope-expired",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.grant(&command).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::InvalidReceipt)
    );

    // Consumed: one receipt authorizes exactly one command.
    let consumed = mint(&pool, team_1(), OWNER, now).await;
    let first = grant_command(
        team_1(),
        consumed.id,
        "scope-consumed-1",
        BusinessRole::Manager,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.grant(&first).await.unwrap(),
        BusinessRoleChangeOutcome::Granted
    );
    let second = revoke_command(
        team_1(),
        consumed.id,
        "scope-consumed-2",
        BusinessRole::Approver,
        OWNER,
        MEMBER,
    );
    assert_eq!(
        svc.revoke(&second).await.unwrap(),
        BusinessRoleChangeOutcome::Denied(RoleChangeDenialReason::InvalidReceipt)
    );

    // No role state changed except the successful grant.
    assert_eq!(
        role_rows(&pool, team_1()).await,
        vec![(MEMBER.into(), "manager".into(), OWNER.into())]
    );
    for request in [
        "scope-cross-team",
        "scope-cross-principal",
        "scope-expired",
        "scope-consumed-2",
    ] {
        let rows = audit_rows(&pool, request).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "denied");
        assert_eq!(rows[0].2.as_deref(), Some("invalid_receipt"));
    }
}
