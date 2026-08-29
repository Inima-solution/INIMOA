use std::collections::HashMap;

use macro_db_migrator::MACRO_DB_MIGRATIONS;
use macro_user_id::{cowlike::CowLike, user_id::MacroUserIdStr};
use model::document::FileType;
use model::folder::{FileSystemNode, FileSystemNodeWithIds, FolderItem};
use model::item::Item;
use model::project::ProjectPreviewV2;
use models_permissions::share_permission::{
    LinkShare, SharePermissionV2, UpdateSharePermissionRequestV2, access_level::AccessLevel,
};
use sqlx::{Pool, Postgres, Row};

use super::PgProjectRepo;
use crate::domain::models::{
    CreateProjectArgs, EditProjectArgs, ProjectOperationalStatus, ProjectPriority,
    ReplaceProjectOperationsArgs, UpdateProjectOperationsCommand, UpdateProjectOperationsOutcome,
    UpdateProjectOperationsRequest, UploadFolderRepoArgs,
};
use crate::domain::ports::ProjectRepo;

const ROOT_ID: &str = "10000000-0000-0000-0000-000000000001";
const CHILD_ID: &str = "10000000-0000-0000-0000-000000000002";
const DELETED_ID: &str = "10000000-0000-0000-0000-000000000009";

#[derive(Debug, Eq, PartialEq)]
struct StoredSharePermission {
    link_share: Option<String>,
    link_share_access_level: Option<String>,
}

async fn project_share_permission_columns(
    pool: &Pool<Postgres>,
    project_id: &str,
) -> StoredSharePermission {
    sqlx::query_as!(
        StoredSharePermission,
        r#"
        SELECT
            permission."linkShare" AS "link_share?",
            permission."linkShareAccessLevel"::text AS "link_share_access_level?"
        FROM "SharePermission" permission
        JOIN "ProjectPermission" project_permission
            ON project_permission."sharePermissionId" = permission.id
        WHERE project_permission."projectId" = $1
        "#,
        project_id,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn history_listing_differs_from_owner_pending_listing(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool);

    let viewed = repo.get_projects_for_user("macro|viewer@test.com").await?;
    assert_eq!(
        viewed
            .iter()
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>(),
        vec!["10000000-0000-0000-0000-000000000005", ROOT_ID]
    );

    let owner_pending = repo
        .get_pending_root_projects("macro|owner@test.com")
        .await?;
    assert_eq!(owner_pending.len(), 1);
    assert_eq!(
        owner_pending[0].project.id,
        "10000000-0000-0000-0000-000000000006"
    );
    assert_eq!(
        owner_pending[0].upload_request_id.as_deref(),
        Some("request-owner")
    );

    let viewer_pending = repo
        .get_pending_root_projects("macro|viewer@test.com")
        .await?;
    assert_eq!(viewer_pending.len(), 1);
    assert_eq!(
        viewer_pending[0].project.id,
        "10000000-0000-0000-0000-000000000007"
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn basic_lookup_includes_deleted_but_full_lookup_excludes_it(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool);

    let basic = repo
        .get_basic_project(DELETED_ID)
        .await?
        .expect("deleted project");
    assert!(basic.deleted_at.is_some());
    assert!(repo.get_project_by_id(DELETED_ID).await?.is_none());
    assert!(repo.get_project_by_id(ROOT_ID).await?.is_some());
    assert!(repo.get_basic_project("missing").await?.is_none());
    assert!(repo.get_project_operations(DELETED_ID).await?.is_none());
    assert!(
        repo.get_project_operations_scoped(DELETED_ID, uuid::Uuid::from_u128(200))
            .await?
            .is_none()
    );
    assert!(repo.get_project_operations("missing").await?.is_none());
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn overview_is_scoped_and_counts_only_live_direct_canonical_children(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let project_id = "10000000-0000-0000-0000-000000000101";
    let child_id = "10000000-0000-0000-0000-000000000102";
    let grandchild_id = "10000000-0000-0000-0000-000000000103";
    let deleted_child_id = "10000000-0000-0000-0000-000000000104";
    let personal_project_id = "10000000-0000-0000-0000-000000000105";
    let personal_owner_id = "macro|overview-personal@test.com";
    let team_id = uuid::Uuid::from_u128(1001);

    sqlx::query(r#"INSERT INTO "team" (id, name, owner_id) VALUES ($1, 'overview', 'macro|owner@test.com')"#)
        .bind(team_id)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO team_user (user_id, team_id, team_role) VALUES ($1, $2, 'owner')")
        .bind("macro|owner@test.com")
        .bind(team_id)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO macro_user (id, username, email, stripe_customer_id) VALUES ($1::uuid, $2, $2, $3)",
    )
    .bind("a3333333-3333-3333-3333-333333333333")
    .bind("overview-personal@test.com")
    .bind("stripe-overview-personal")
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO \"User\" (id, email, \"stripeCustomerId\", macro_user_id) VALUES ($1, $2, $3, $4::uuid)")
        .bind(personal_owner_id)
        .bind("overview-personal@test.com")
        .bind("stripe-overview-personal")
        .bind("a3333333-3333-3333-3333-333333333333")
        .execute(&pool)
        .await?;
    for (id, parent_id, deleted_at, owner_id) in [
        (project_id, None, None, "macro|owner@test.com"),
        (child_id, Some(project_id), None, "macro|owner@test.com"),
        (grandchild_id, Some(child_id), None, "macro|owner@test.com"),
        (
            deleted_child_id,
            Some(project_id),
            Some("2026-08-29"),
            "macro|owner@test.com",
        ),
        (personal_project_id, None, None, personal_owner_id),
    ] {
        sqlx::query(
            r#"INSERT INTO "Project" (id, name, "userId", "parentId", "deletedAt") VALUES ($1, $2, $3, $4, $5::timestamp)"#,
        )
        .bind(id)
        .bind(format!("overview-{id}"))
        .bind(owner_id)
        .bind(parent_id)
        .bind(deleted_at)
        .execute(&pool)
        .await?;
    }

    let documents = [
        (
            "20000000-0000-0000-0000-000000000101",
            "ordinary",
            Some("pdf"),
            project_id,
            None,
        ),
        (
            "20000000-0000-0000-0000-000000000102",
            "task",
            None,
            project_id,
            None,
        ),
        (
            "20000000-0000-0000-0000-000000000103",
            "snippet",
            None,
            project_id,
            None,
        ),
        (
            "20000000-0000-0000-0000-000000000104",
            "skill",
            Some("md"),
            project_id,
            None,
        ),
        (
            "20000000-0000-0000-0000-000000000105",
            "deleted",
            Some("pdf"),
            project_id,
            Some("2026-08-29"),
        ),
        (
            "20000000-0000-0000-0000-000000000106",
            "nested",
            Some("pdf"),
            child_id,
            None,
        ),
        (
            "20000000-0000-0000-0000-000000000107",
            "deleted-task",
            None,
            project_id,
            Some("2026-08-29"),
        ),
        (
            "20000000-0000-0000-0000-000000000108",
            "nested-task",
            None,
            child_id,
            None,
        ),
    ];
    for (id, name, file_type, document_project_id, deleted_at) in documents {
        sqlx::query(
            r#"INSERT INTO "Document" (id, name, owner, "fileType", "projectId", "deletedAt") VALUES ($1, $2, 'macro|owner@test.com', $3, $4, $5::timestamp)"#,
        )
        .bind(id)
        .bind(name)
        .bind(file_type)
        .bind(document_project_id)
        .bind(deleted_at)
        .execute(&pool)
        .await?;
    }
    for (document_id, sub_type) in [
        ("20000000-0000-0000-0000-000000000102", "task"),
        ("20000000-0000-0000-0000-000000000103", "snippet"),
        ("20000000-0000-0000-0000-000000000104", "skill"),
        ("20000000-0000-0000-0000-000000000107", "task"),
        ("20000000-0000-0000-0000-000000000108", "task"),
    ] {
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, $2::document_sub_type_value)")
            .bind(document_id)
            .bind(sub_type)
            .execute(&pool)
            .await?;
    }
    for (id, chat_project_id, deleted_at) in [
        ("30000000-0000-0000-0000-000000000101", project_id, None),
        (
            "30000000-0000-0000-0000-000000000102",
            project_id,
            Some("2026-08-29"),
        ),
        ("30000000-0000-0000-0000-000000000103", child_id, None),
    ] {
        sqlx::query(
            r#"INSERT INTO "Chat" (id, name, "userId", "projectId", "deletedAt") VALUES ($1, 'overview', 'macro|owner@test.com', $2, $3::timestamp)"#,
        )
        .bind(id)
        .bind(chat_project_id)
        .bind(deleted_at)
        .execute(&pool)
        .await?;
    }
    sqlx::query(
        "UPDATE project_operations SET status = 'active', priority = 'high', lead_user_id = $1, start_date = DATE '2026-08-01', target_date = DATE '2026-08-31', policy = '{\"source\":\"overview-test\"}'::jsonb, created_at = TIMESTAMPTZ '2026-08-01 01:00:00Z', updated_at = TIMESTAMPTZ '2026-08-02 01:00:00Z' WHERE project_id = $2",
    )
        .bind("macro|viewer@test.com")
        .bind(project_id)
        .execute(&pool)
        .await?;

    let overview = repo
        .get_project_overview_scoped(project_id, team_id)
        .await?
        .expect("scoped overview");
    assert_eq!(overview.project.id, project_id);
    assert_eq!(overview.project.name, format!("overview-{project_id}"));
    assert_eq!(overview.project.user_id, "macro|owner@test.com");
    assert!(overview.project.parent_id.is_none());
    assert_eq!(overview.operations.project_id, project_id);
    assert!(overview.operations.lead_user_id.is_none());
    assert_eq!(overview.operations.status, ProjectOperationalStatus::Active);
    assert_eq!(overview.operations.priority, ProjectPriority::High);
    assert_eq!(
        overview.operations.start_date.unwrap().to_string(),
        "2026-08-01"
    );
    assert_eq!(
        overview.operations.target_date.unwrap().to_string(),
        "2026-08-31"
    );
    assert_eq!(
        overview.operations.policy,
        Some(serde_json::json!({"source": "overview-test"}))
    );
    assert!(overview.operations.completed_at.is_none());
    assert_eq!(
        overview.operations.created_at.to_rfc3339(),
        "2026-08-01T01:00:00+00:00"
    );
    assert_eq!(
        overview.operations.updated_at.to_rfc3339(),
        "2026-08-02T01:00:00+00:00"
    );
    assert_eq!(overview.immediate_children.child_projects, 1);
    assert_eq!(overview.immediate_children.tasks, 1);
    assert_eq!(overview.immediate_children.non_task_documents, 3);
    assert_eq!(overview.immediate_children.chats, 1);
    assert!(
        repo.get_project_overview_scoped(project_id, uuid::Uuid::from_u128(1002))
            .await?
            .is_none()
    );
    assert!(
        repo.get_project_overview_scoped(DELETED_ID, team_id)
            .await?
            .is_none()
    );
    assert!(
        repo.get_project_overview_scoped("missing", team_id)
            .await?
            .is_none()
    );
    assert!(
        repo.get_project_overview_scoped(personal_project_id, team_id)
            .await?
            .is_none()
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn operations_update_is_team_scoped_audited_and_noop_is_not_audited(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let team_id = uuid::Uuid::from_u128(201);
    sqlx::query(r#"INSERT INTO "team" (id, name, owner_id) VALUES ($1, 'operations', 'macro|owner@test.com')"#)
        .bind(team_id).execute(&pool).await?;
    sqlx::query("INSERT INTO team_user (user_id, team_id, team_role) VALUES ($1, $2, 'owner')")
        .bind("macro|owner@test.com")
        .bind(team_id)
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE project_operations SET lead_user_id = $1 WHERE project_id = $2")
        .bind("macro|viewer@test.com")
        .bind(ROOT_ID)
        .execute(&pool)
        .await?;
    assert!(
        repo.get_project_operations_scoped(ROOT_ID, team_id)
            .await?
            .unwrap()
            .lead_user_id
            .is_none()
    );
    assert!(
        repo.get_project_operations_scoped(ROOT_ID, team_id)
            .await?
            .is_some()
    );
    assert!(
        repo.get_project_operations_scoped(ROOT_ID, uuid::Uuid::from_u128(202))
            .await?
            .is_none()
    );
    let current = repo.get_project_operations(ROOT_ID).await?.unwrap();
    assert!(matches!(
        repo.update_project_operations(UpdateProjectOperationsCommand {
            team_id: uuid::Uuid::from_u128(202),
            actor_user_id: MacroUserIdStr::parse_from_str("macro|owner@test.com")?.into_owned(),
            request: UpdateProjectOperationsRequest {
                project_id: ROOT_ID.to_owned(),
                request_id: "operations-cross-team".to_owned(),
                now: chrono::Utc::now(),
                replacement: ReplaceProjectOperationsArgs {
                    status: ProjectOperationalStatus::Active,
                    priority: current.priority,
                    lead_user_id: None,
                    start_date: current.start_date,
                    target_date: current.target_date,
                    policy: current.policy.clone(),
                    expected_updated_at: current.updated_at,
                },
            },
        })
        .await?,
        UpdateProjectOperationsOutcome::NotFound
    ));
    assert_eq!(
        repo.get_project_operations(ROOT_ID)
            .await?
            .unwrap()
            .updated_at,
        current.updated_at
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM business_audit_events")
            .fetch_one(&pool)
            .await?,
        0
    );
    let now = chrono::Utc::now();
    let command = UpdateProjectOperationsCommand {
        team_id,
        actor_user_id: MacroUserIdStr::parse_from_str("macro|owner@test.com")?.into_owned(),
        request: UpdateProjectOperationsRequest {
            project_id: ROOT_ID.to_owned(),
            request_id: "operations-test".to_owned(),
            now,
            replacement: ReplaceProjectOperationsArgs {
                status: ProjectOperationalStatus::Active,
                priority: ProjectPriority::High,
                lead_user_id: None,
                start_date: None,
                target_date: None,
                policy: None,
                expected_updated_at: current.updated_at,
            },
        },
    };
    let updated = repo.update_project_operations(command).await?;
    assert!(
        matches!(updated, UpdateProjectOperationsOutcome::Updated(ref row) if row.status == ProjectOperationalStatus::Active && row.priority == ProjectPriority::High)
    );
    let audit = sqlx::query("SELECT team_id, actor, delegated_actor, action, target_type, target_id, outcome, request_id, reason, retention_class, metadata FROM business_audit_events WHERE action = 'project_operations_updated'").fetch_one(&pool).await?;
    assert_eq!(audit.get::<uuid::Uuid, _>("team_id"), team_id);
    assert_eq!(audit.get::<String, _>("actor"), "macro|owner@test.com");
    assert_eq!(
        audit.get::<String, _>("action"),
        "project_operations_updated"
    );
    assert_eq!(audit.get::<String, _>("target_type"), "project");
    assert_eq!(audit.get::<String, _>("target_id"), ROOT_ID);
    assert_eq!(audit.get::<String, _>("outcome"), "success");
    assert_eq!(audit.get::<String, _>("request_id"), "operations-test");
    assert_eq!(audit.get::<String, _>("retention_class"), "standard");
    assert!(audit.get::<Option<String>, _>("delegated_actor").is_none());
    assert!(audit.get::<Option<String>, _>("reason").is_none());
    let metadata: serde_json::Value = audit.get("metadata");
    assert_eq!(metadata["from_status"], "planned");
    assert_eq!(metadata["to_status"], "active");
    assert_eq!(
        metadata["changed_fields"],
        serde_json::json!(["status", "priority", "lead_user_id"])
    );
    assert!(metadata.get("lead_user_id").is_none());
    let after = repo.get_project_operations(ROOT_ID).await?.unwrap();
    let noop = UpdateProjectOperationsCommand {
        team_id,
        actor_user_id: MacroUserIdStr::parse_from_str("macro|owner@test.com")?.into_owned(),
        request: UpdateProjectOperationsRequest {
            project_id: ROOT_ID.to_owned(),
            request_id: "operations-noop".to_owned(),
            now: chrono::Utc::now(),
            replacement: ReplaceProjectOperationsArgs {
                status: after.status,
                priority: after.priority,
                lead_user_id: after.lead_user_id.clone(),
                start_date: after.start_date,
                target_date: after.target_date,
                policy: after.policy.clone(),
                expected_updated_at: chrono::Utc::now(),
            },
        },
    };
    assert!(matches!(
        repo.update_project_operations(noop).await?,
        UpdateProjectOperationsOutcome::Unchanged(_)
    ));
    let audits_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM business_audit_events WHERE action = 'project_operations_updated'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(audits_after, 1);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn operations_rejects_invalid_stale_and_departed_lead_without_audit_or_partial_write(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let team_id = uuid::Uuid::from_u128(211);
    sqlx::query(r#"INSERT INTO "team" (id, name, owner_id) VALUES ($1, 'operations-guard', 'macro|owner@test.com')"#).bind(team_id).execute(&pool).await?;
    sqlx::query("INSERT INTO team_user (user_id, team_id, team_role) VALUES ($1, $2, 'owner')")
        .bind("macro|owner@test.com")
        .bind(team_id)
        .execute(&pool)
        .await?;
    let before = repo.get_project_operations(ROOT_ID).await?.unwrap();
    let command = |status, lead, expected| UpdateProjectOperationsCommand {
        team_id,
        actor_user_id: MacroUserIdStr::parse_from_str("macro|owner@test.com")
            .unwrap()
            .into_owned(),
        request: UpdateProjectOperationsRequest {
            project_id: ROOT_ID.to_owned(),
            request_id: "operations-guard".to_owned(),
            now: chrono::Utc::now(),
            replacement: ReplaceProjectOperationsArgs {
                status,
                priority: before.priority,
                lead_user_id: lead,
                start_date: None,
                target_date: None,
                policy: None,
                expected_updated_at: expected,
            },
        },
    };
    assert!(matches!(
        repo.update_project_operations(command(
            ProjectOperationalStatus::Paused,
            None,
            before.updated_at
        ))
        .await?,
        UpdateProjectOperationsOutcome::Invalid(_)
    ));
    assert!(matches!(
        repo.update_project_operations(command(
            ProjectOperationalStatus::Active,
            None,
            chrono::Utc::now()
        ))
        .await?,
        UpdateProjectOperationsOutcome::Conflict
    ));
    let departed = MacroUserIdStr::parse_from_str("macro|viewer@test.com")?.into_owned();
    assert!(matches!(
        repo.update_project_operations(command(
            ProjectOperationalStatus::Active,
            Some(departed),
            before.updated_at
        ))
        .await?,
        UpdateProjectOperationsOutcome::LeadNotInOwnerTeam
    ));
    for replacement in [
        ReplaceProjectOperationsArgs {
            status: ProjectOperationalStatus::Planned,
            priority: before.priority,
            lead_user_id: None,
            start_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap()),
            target_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()),
            policy: None,
            expected_updated_at: before.updated_at,
        },
        ReplaceProjectOperationsArgs {
            status: ProjectOperationalStatus::Planned,
            priority: before.priority,
            lead_user_id: None,
            start_date: None,
            target_date: None,
            policy: Some(serde_json::json!([])),
            expected_updated_at: before.updated_at,
        },
        ReplaceProjectOperationsArgs {
            status: ProjectOperationalStatus::Planned,
            priority: before.priority,
            lead_user_id: None,
            start_date: None,
            target_date: None,
            policy: Some(serde_json::json!({"value": "x".repeat(4096)})),
            expected_updated_at: before.updated_at,
        },
    ] {
        assert!(matches!(
            repo.update_project_operations(UpdateProjectOperationsCommand {
                team_id,
                actor_user_id: MacroUserIdStr::parse_from_str("macro|owner@test.com")?.into_owned(),
                request: UpdateProjectOperationsRequest {
                    project_id: ROOT_ID.to_owned(),
                    request_id: "operations-invalid".to_owned(),
                    now: chrono::Utc::now(),
                    replacement,
                },
            })
            .await?,
            UpdateProjectOperationsOutcome::Invalid(_)
        ));
    }
    assert_eq!(
        repo.get_project_operations(ROOT_ID)
            .await?
            .unwrap()
            .updated_at,
        before.updated_at
    );
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM business_audit_events WHERE action = 'project_operations_updated'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(audit_count, 0);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn operations_without_an_active_owner_team_are_not_scoped_or_mutable(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let project_id = "10000000-0000-0000-0000-000000000007";
    let before = repo.get_project_operations(project_id).await?.unwrap();
    let team_id = uuid::Uuid::from_u128(212);
    assert!(
        repo.get_project_operations_scoped(project_id, team_id)
            .await?
            .is_none()
    );
    assert!(matches!(
        repo.update_project_operations(UpdateProjectOperationsCommand {
            team_id,
            actor_user_id: MacroUserIdStr::parse_from_str("macro|viewer@test.com")?.into_owned(),
            request: UpdateProjectOperationsRequest {
                project_id: project_id.to_owned(),
                request_id: "operations-personal".to_owned(),
                now: chrono::Utc::now(),
                replacement: ReplaceProjectOperationsArgs {
                    status: ProjectOperationalStatus::Active,
                    priority: before.priority,
                    lead_user_id: None,
                    start_date: before.start_date,
                    target_date: before.target_date,
                    policy: before.policy.clone(),
                    expected_updated_at: before.updated_at,
                },
            },
        })
        .await?,
        UpdateProjectOperationsOutcome::NotFound
    ));
    assert_eq!(
        repo.get_project_operations(project_id)
            .await?
            .unwrap()
            .updated_at,
        before.updated_at
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM business_audit_events")
            .fetch_one(&pool)
            .await?,
        0
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn operations_audit_failure_rolls_back_updated_row(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let team_id = uuid::Uuid::from_u128(221);
    sqlx::query(r#"INSERT INTO "team" (id, name, owner_id) VALUES ($1, 'operations-rollback', 'macro|owner@test.com')"#).bind(team_id).execute(&pool).await?;
    sqlx::query("INSERT INTO team_user (user_id, team_id, team_role) VALUES ($1, $2, 'owner')")
        .bind("macro|owner@test.com")
        .bind(team_id)
        .execute(&pool)
        .await?;
    let before = repo.get_project_operations(ROOT_ID).await?.unwrap();
    let result = repo
        .update_project_operations(UpdateProjectOperationsCommand {
            team_id,
            actor_user_id: MacroUserIdStr::parse_from_str("macro|owner@test.com")?.into_owned(),
            request: UpdateProjectOperationsRequest {
                project_id: ROOT_ID.to_owned(),
                request_id: "x".repeat(257),
                now: chrono::Utc::now(),
                replacement: ReplaceProjectOperationsArgs {
                    status: ProjectOperationalStatus::Active,
                    priority: before.priority,
                    lead_user_id: None,
                    start_date: None,
                    target_date: None,
                    policy: None,
                    expected_updated_at: before.updated_at,
                },
            },
        })
        .await;
    assert!(result.is_err());
    let after = repo.get_project_operations(ROOT_ID).await?.unwrap();
    assert_eq!(after.status, before.status);
    assert_eq!(after.updated_at, before.updated_at);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn operations_concurrent_replacements_have_one_winner_and_idempotent_retry(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let team_id = uuid::Uuid::from_u128(231);
    sqlx::query(r#"INSERT INTO "team" (id, name, owner_id) VALUES ($1, 'operations-concurrent', 'macro|owner@test.com')"#).bind(team_id).execute(&pool).await?;
    sqlx::query("INSERT INTO team_user (user_id, team_id, team_role) VALUES ($1, $2, 'owner')")
        .bind("macro|owner@test.com")
        .bind(team_id)
        .execute(&pool)
        .await?;
    let before = repo.get_project_operations(ROOT_ID).await?.unwrap();
    let make = |priority| UpdateProjectOperationsCommand {
        team_id,
        actor_user_id: MacroUserIdStr::parse_from_str("macro|owner@test.com")
            .unwrap()
            .into_owned(),
        request: UpdateProjectOperationsRequest {
            project_id: ROOT_ID.to_owned(),
            request_id: format!("concurrent-{priority}"),
            now: chrono::Utc::now(),
            replacement: ReplaceProjectOperationsArgs {
                status: ProjectOperationalStatus::Active,
                priority,
                lead_user_id: None,
                start_date: None,
                target_date: None,
                policy: None,
                expected_updated_at: before.updated_at,
            },
        },
    };
    let left_repo = repo.clone();
    let right_repo = repo.clone();
    let (left, right) = tokio::join!(
        left_repo.update_project_operations(make(ProjectPriority::High)),
        right_repo.update_project_operations(make(ProjectPriority::Urgent))
    );
    let outcomes = [left?, right?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, UpdateProjectOperationsOutcome::Updated(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, UpdateProjectOperationsOutcome::Conflict))
            .count(),
        1
    );
    let current = repo.get_project_operations(ROOT_ID).await?.unwrap();
    let retry = UpdateProjectOperationsCommand {
        team_id,
        actor_user_id: MacroUserIdStr::parse_from_str("macro|owner@test.com")?.into_owned(),
        request: UpdateProjectOperationsRequest {
            project_id: ROOT_ID.to_owned(),
            request_id: "identical-stale".to_owned(),
            now: chrono::Utc::now(),
            replacement: ReplaceProjectOperationsArgs {
                status: current.status,
                priority: current.priority,
                lead_user_id: None,
                start_date: current.start_date,
                target_date: current.target_date,
                policy: current.policy.clone(),
                expected_updated_at: before.updated_at,
            },
        },
    };
    assert!(matches!(
        repo.update_project_operations(retry).await?,
        UpdateProjectOperationsOutcome::Unchanged(_)
    ));
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM business_audit_events WHERE action = 'project_operations_updated'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(audits, 1);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn project_operations_backfill_and_insert_trigger_provision_defaults(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let canonical_project_name: String =
        sqlx::query_scalar(r#"SELECT name FROM "Project" WHERE id = $1"#)
            .bind(ROOT_ID)
            .fetch_one(&pool)
            .await?;
    assert_eq!(canonical_project_name, "Root");

    sqlx::raw_sql(include_str!(
        "../../../../macro_db_client/migrations/20260828210000_project_operations.down.sql"
    ))
    .execute(&pool)
    .await?;

    let operations_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('project_operations')::text")
            .fetch_one(&pool)
            .await?;
    assert!(operations_table.is_none());

    let preexisting_project_id = "10000000-0000-0000-0000-000000000010";
    sqlx::query(
        r#"
        INSERT INTO "Project" (id, name, "userId", "createdAt", "updatedAt")
        VALUES ($1, $2, $3, NOW(), NOW())
        "#,
    )
    .bind(preexisting_project_id)
    .bind("Backfill source")
    .bind("macro|owner@test.com")
    .execute(&pool)
    .await?;

    sqlx::raw_sql(include_str!(
        "../../../../macro_db_client/migrations/20260828210000_project_operations.up.sql"
    ))
    .execute(&pool)
    .await?;

    let repo = PgProjectRepo::new(pool.clone());
    let backfilled = repo
        .get_project_operations(preexisting_project_id)
        .await?
        .expect("operations backfilled for the preexisting project");
    assert_eq!(backfilled.status.to_string(), "planned");
    assert_eq!(backfilled.priority.to_string(), "normal");
    assert!(backfilled.lead_user_id.is_none());
    assert!(backfilled.start_date.is_none());
    assert!(backfilled.target_date.is_none());
    assert!(backfilled.completed_at.is_none());
    assert!(backfilled.policy.is_none());

    let trigger_project_id = "10000000-0000-0000-0000-000000000011";
    sqlx::query(
        r#"
        INSERT INTO "Project" (id, name, "userId", "createdAt", "updatedAt")
        VALUES ($1, $2, $3, NOW(), NOW())
        "#,
    )
    .bind(trigger_project_id)
    .bind("Trigger source")
    .bind("macro|owner@test.com")
    .execute(&pool)
    .await?;
    let operations = repo
        .get_project_operations(trigger_project_id)
        .await?
        .expect("operations provisioned by trigger");
    assert_eq!(operations.status.to_string(), "planned");
    assert_eq!(operations.priority.to_string(), "normal");
    assert!(operations.lead_user_id.is_none());
    assert!(operations.start_date.is_none());
    assert!(operations.target_date.is_none());
    assert!(operations.completed_at.is_none());
    assert!(operations.policy.is_none());
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn project_operations_database_constraints_reject_invalid_values_and_null_deleted_leads(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    for statement in [
        "UPDATE project_operations SET status = 'unknown' WHERE project_id = $1",
        "UPDATE project_operations SET priority = 'rush' WHERE project_id = $1",
        "UPDATE project_operations SET start_date = DATE '2026-02-02', target_date = DATE '2026-02-01' WHERE project_id = $1",
        "UPDATE project_operations SET policy = '[]'::jsonb WHERE project_id = $1",
        "UPDATE project_operations SET policy = jsonb_build_object('value', repeat('x', 4096)) WHERE project_id = $1",
        "UPDATE project_operations SET lead_user_id = 'macro|missing@test.com' WHERE project_id = $1",
    ] {
        assert!(
            sqlx::query(statement)
                .bind(ROOT_ID)
                .execute(&pool)
                .await
                .is_err()
        );
    }

    let lead = "macro|operations-lead@test.com";
    let macro_user_id = "f0000000-0000-0000-0000-000000000001";
    sqlx::query(
        "INSERT INTO macro_user (id, username, email, stripe_customer_id) VALUES ($1::uuid, $2, $3, $4)",
    )
    .bind(macro_user_id)
    .bind("operations-lead")
    .bind("operations-lead@test.com")
    .bind("cus_operations_lead")
    .execute(&pool)
    .await?;
    sqlx::query(r#"INSERT INTO "User" (id, email, macro_user_id) VALUES ($1, $2, $3::uuid)"#)
        .bind(lead)
        .bind("operations-lead@test.com")
        .bind(macro_user_id)
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE project_operations SET lead_user_id = $1 WHERE project_id = $2")
        .bind(lead)
        .bind(ROOT_ID)
        .execute(&pool)
        .await?;
    sqlx::query(r#"DELETE FROM "User" WHERE id = $1"#)
        .bind(lead)
        .execute(&pool)
        .await?;
    let row = sqlx::query("SELECT lead_user_id FROM project_operations WHERE project_id = $1")
        .bind(ROOT_ID)
        .fetch_one(&pool)
        .await?;
    assert!(row.try_get::<Option<String>, _>("lead_user_id")?.is_none());
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn project_operations_survive_parent_move_soft_delete_restore_and_project_deletion(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let before_move = repo
        .get_project_operations(CHILD_ID)
        .await?
        .expect("child operations");
    repo.edit_project(EditProjectArgs {
        project_id: CHILD_ID.to_owned(),
        name: None,
        update_parent: true,
        parent_id: None,
        share_permission: None,
    })
    .await?;
    assert_eq!(
        repo.get_project_operations(CHILD_ID).await?,
        Some(before_move)
    );

    let before_delete = repo
        .get_project_operations(CHILD_ID)
        .await?
        .expect("active child operations");
    repo.soft_delete_project(CHILD_ID).await?;
    assert!(repo.get_project_operations(CHILD_ID).await?.is_none());
    let retained: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_operations WHERE project_id = $1")
            .bind(CHILD_ID)
            .fetch_one(&pool)
            .await?;
    assert_eq!(retained, 1);
    repo.revert_delete_project(CHILD_ID, None).await?;
    assert_eq!(
        repo.get_project_operations(CHILD_ID).await?,
        Some(before_delete)
    );

    let transient = repo
        .create_project(CreateProjectArgs {
            user_id: "macro|owner@test.com".to_owned(),
            name: "Compensation cascade".to_owned(),
            parent_id: None,
            share_permission: SharePermissionV2::new_project_share_permission(None),
        })
        .await?;
    repo.delete_uploaded_tree(&[transient.id.clone()], &[])
        .await?;
    let cascaded: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_operations WHERE project_id = $1")
            .bind(&transient.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(cascaded, 0);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn children_are_depth_one_filtered_and_type_ordered(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool);
    let children = repo.get_project_children(ROOT_ID).await?;

    let children = children
        .into_iter()
        .map(|item| match item {
            Item::Project(project) => ("project", project.id),
            Item::Document(document) => ("document", document.document_id),
            Item::Chat(chat) => ("chat", chat.id),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        children,
        vec![
            ("project", CHILD_ID.to_owned()),
            (
                "document",
                "20000000-0000-0000-0000-000000000001".to_owned()
            ),
            ("chat", "30000000-0000-0000-0000-000000000001".to_owned()),
        ]
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn preview_preserves_found_and_missing_input_entries(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool);
    let missing = "10000000-0000-0000-0000-000000000099".to_owned();
    let previews = repo
        .batch_get_project_preview(&[CHILD_ID.to_owned(), missing.clone()])
        .await?;

    match &previews[0] {
        ProjectPreviewV2::Found(project) => {
            assert_eq!(project.id, CHILD_ID);
            assert_eq!(project.path, vec!["Root", "First child"]);
        }
        ProjectPreviewV2::DoesNotExist(_) => panic!("child should exist"),
    }
    match &previews[1] {
        ProjectPreviewV2::DoesNotExist(project) => assert_eq!(project.id, missing),
        ProjectPreviewV2::Found(_) => panic!("missing project should not be found"),
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn reads_share_permissions_and_bumps_modified_timestamp(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool);
    let permission = repo.get_project_share_permission(ROOT_ID).await?;
    assert_eq!(permission.id, "share-root");
    assert_eq!(permission.owner, "macro|owner@test.com");
    assert_eq!(permission.link_share, Some(LinkShare::Public));
    assert_eq!(permission.link_share_access_level, Some(AccessLevel::Edit));
    assert_eq!(
        permission.channel_share_permissions.expect("channel").len(),
        1
    );

    let before = repo
        .get_project_by_id(ROOT_ID)
        .await?
        .expect("root")
        .updated_at;
    repo.update_project_modified(ROOT_ID).await?;
    let after = repo
        .get_project_by_id(ROOT_ID)
        .await?
        .expect("root")
        .updated_at;
    assert!(after > before);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn create_is_atomic_and_inserts_all_metadata(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let permission = SharePermissionV2::new_project_share_permission(None);
    let project = repo
        .create_project(CreateProjectArgs {
            user_id: "macro|owner@test.com".to_owned(),
            name: "Created".to_owned(),
            parent_id: Some(ROOT_ID.to_owned()),
            share_permission: permission.clone(),
        })
        .await?;

    let metadata_count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) AS "count!"
        FROM "ProjectPermission" permission
        JOIN "UserHistory" history ON history."itemId" = permission."projectId"
        JOIN entity_access access ON access.entity_id::text = permission."projectId"
        WHERE permission."projectId" = $1
          AND history."itemType" = 'project'
          AND access.access_level = 'owner'
        "#,
        project.id,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(metadata_count, 1);
    assert_eq!(
        project_share_permission_columns(&pool, &project.id).await,
        StoredSharePermission {
            link_share: None,
            link_share_access_level: None,
        }
    );

    assert!(
        repo.create_project(CreateProjectArgs {
            user_id: "macro|owner@test.com".to_owned(),
            name: "Must roll back".to_owned(),
            parent_id: Some("missing-parent".to_owned()),
            share_permission: permission,
        })
        .await
        .is_err()
    );
    let rolled_back = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM "Project" WHERE name = 'Must roll back'"#
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(rolled_back, 0);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn create_defaults_enabled_link_share_to_view(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let mut permission = SharePermissionV2::new_project_share_permission(None);
    permission.link_share = Some(LinkShare::Team);

    let project = repo
        .create_project(CreateProjectArgs {
            user_id: "macro|owner@test.com".to_owned(),
            name: "Team project".to_owned(),
            parent_id: None,
            share_permission: permission,
        })
        .await?;

    assert_eq!(
        project_share_permission_columns(&pool, &project.id).await,
        StoredSharePermission {
            link_share: Some("TEAM".to_owned()),
            link_share_access_level: Some("view".to_owned()),
        }
    );
    let permission = repo.get_project_share_permission(&project.id).await?;
    assert_eq!(permission.link_share, Some(LinkShare::Team));
    assert_eq!(permission.link_share_access_level, Some(AccessLevel::View));
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn edit_supports_parent_flags_and_sharing(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let unchanged = repo
        .edit_project(EditProjectArgs {
            project_id: CHILD_ID.to_owned(),
            name: Some("Renamed".to_owned()),
            update_parent: false,
            parent_id: None,
            share_permission: None,
        })
        .await?;
    assert_eq!(unchanged.parent_id.as_deref(), Some(ROOT_ID));

    let moved = repo
        .edit_project(EditProjectArgs {
            project_id: CHILD_ID.to_owned(),
            name: None,
            update_parent: true,
            parent_id: Some("10000000-0000-0000-0000-000000000005".to_owned()),
            share_permission: None,
        })
        .await?;
    assert_eq!(
        moved.parent_id.as_deref(),
        Some("10000000-0000-0000-0000-000000000005")
    );

    let updated = repo
        .edit_project(EditProjectArgs {
            project_id: ROOT_ID.to_owned(),
            name: None,
            update_parent: true,
            parent_id: None,
            share_permission: Some(UpdateSharePermissionRequestV2 {
                link_share: Some(Some(LinkShare::Team)),
                link_share_access_level: Some(None),
                channel_share_permissions: None,
            }),
        })
        .await?;
    assert!(updated.parent_id.is_none());
    assert_eq!(
        project_share_permission_columns(&pool, ROOT_ID).await,
        StoredSharePermission {
            link_share: Some("TEAM".to_owned()),
            link_share_access_level: Some("view".to_owned()),
        }
    );

    repo.edit_project(EditProjectArgs {
        project_id: ROOT_ID.to_owned(),
        name: None,
        update_parent: false,
        parent_id: None,
        share_permission: Some(UpdateSharePermissionRequestV2 {
            link_share: None,
            link_share_access_level: Some(Some(AccessLevel::Comment)),
            channel_share_permissions: None,
        }),
    })
    .await?;
    assert_eq!(
        project_share_permission_columns(&pool, ROOT_ID).await,
        StoredSharePermission {
            link_share: Some("TEAM".to_owned()),
            link_share_access_level: Some("comment".to_owned()),
        }
    );

    repo.edit_project(EditProjectArgs {
        project_id: ROOT_ID.to_owned(),
        name: None,
        update_parent: false,
        parent_id: None,
        share_permission: Some(UpdateSharePermissionRequestV2 {
            link_share: Some(Some(LinkShare::Public)),
            link_share_access_level: Some(Some(AccessLevel::Edit)),
            channel_share_permissions: None,
        }),
    })
    .await?;
    let before_omitted_update = project_share_permission_columns(&pool, ROOT_ID).await;
    assert_eq!(
        before_omitted_update,
        StoredSharePermission {
            link_share: Some("PUBLIC".to_owned()),
            link_share_access_level: Some("edit".to_owned()),
        }
    );

    repo.edit_project(EditProjectArgs {
        project_id: ROOT_ID.to_owned(),
        name: None,
        update_parent: false,
        parent_id: None,
        share_permission: Some(UpdateSharePermissionRequestV2 {
            link_share: None,
            link_share_access_level: None,
            channel_share_permissions: None,
        }),
    })
    .await?;
    assert_eq!(
        project_share_permission_columns(&pool, ROOT_ID).await,
        before_omitted_update
    );

    repo.edit_project(EditProjectArgs {
        project_id: ROOT_ID.to_owned(),
        name: None,
        update_parent: false,
        parent_id: None,
        share_permission: Some(UpdateSharePermissionRequestV2 {
            link_share: Some(None),
            link_share_access_level: Some(Some(AccessLevel::Edit)),
            channel_share_permissions: None,
        }),
    })
    .await?;
    assert_eq!(
        project_share_permission_columns(&pool, ROOT_ID).await,
        StoredSharePermission {
            link_share: None,
            link_share_access_level: None,
        }
    );
    let permission = repo.get_project_share_permission(ROOT_ID).await?;
    assert_eq!(permission.link_share, None);
    assert_eq!(permission.link_share_access_level, None);
    assert_eq!(
        permission.channel_share_permissions.expect("channel").len(),
        1
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn recursive_detection_and_soft_delete_output(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    assert!(repo.is_project_recursively_nested(ROOT_ID, ROOT_ID).await?);
    assert!(
        repo.is_project_recursively_nested(ROOT_ID, CHILD_ID)
            .await?
    );
    assert!(
        !repo
            .is_project_recursively_nested(CHILD_ID, "10000000-0000-0000-0000-000000000005")
            .await?
    );

    let result = repo.soft_delete_project(ROOT_ID).await?;
    assert_eq!(result.project_ids.len(), 3);
    assert_eq!(result.document_ids.len(), 2);
    assert_eq!(result.chat_ids.len(), 2);
    let remaining_history = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM "UserHistory" WHERE "itemId" = $1"#,
        ROOT_ID,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(remaining_history, 0);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn subtree_delete_and_restore_serialize_behind_taskdeps_lock(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let mut delete_lock = pool.begin().await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(&mut *delete_lock)
    .await?;
    let delete_repo = repo.clone();
    let mut delete = tokio::spawn(async move { delete_repo.soft_delete_project(ROOT_ID).await });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(25), &mut delete)
            .await
            .is_err()
    );
    delete_lock.commit().await?;
    let deleted = delete.await??;
    assert!(deleted.project_ids.iter().any(|id| id == ROOT_ID));
    assert!(repo.get_project_by_id(ROOT_ID).await?.is_none());

    let mut restore_lock = pool.begin().await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(&mut *restore_lock)
    .await?;
    let restore_repo = repo.clone();
    let mut restore =
        tokio::spawn(async move { restore_repo.revert_delete_project(ROOT_ID, None).await });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(25), &mut restore)
            .await
            .is_err()
    );
    restore_lock.commit().await?;
    let restored = restore.await??;
    assert!(restored.project_ids.iter().any(|id| id == ROOT_ID));
    assert!(repo.get_project_by_id(ROOT_ID).await?.is_some());
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn revert_restores_subtree_and_handles_parent_state(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    sqlx::query!(
        r#"UPDATE "Project" SET "parentId" = $2 WHERE id = $1"#,
        ROOT_ID,
        DELETED_ID
    )
    .execute(&pool)
    .await?;
    repo.soft_delete_project(ROOT_ID).await?;
    let restored = repo
        .revert_delete_project(ROOT_ID, Some(DELETED_ID.to_owned()))
        .await?;
    assert_eq!(restored.project_ids.len(), 4);
    assert!(
        repo.get_basic_project(ROOT_ID)
            .await?
            .expect("root")
            .parent_id
            .is_none()
    );
    assert!(repo.get_project_by_id(CHILD_ID).await?.is_some());

    repo.soft_delete_project(CHILD_ID).await?;
    repo.revert_delete_project(CHILD_ID, Some(ROOT_ID.to_owned()))
        .await?;
    assert_eq!(
        repo.get_basic_project(CHILD_ID)
            .await?
            .expect("child")
            .parent_id
            .as_deref(),
        Some(ROOT_ID)
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn purge_returns_outputs_and_removes_access_and_permissions(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let deleted = repo.soft_delete_project(ROOT_ID).await?;
    let document_id = &deleted.document_ids[0];
    let bom_id = sqlx::query_scalar!(
        r#"INSERT INTO "DocumentBom" ("documentId") VALUES ($1) RETURNING id"#,
        document_id,
    )
    .fetch_one(&pool)
    .await?;
    sqlx::query!(
        r#"
        INSERT INTO "BomPart" (sha, path, "documentBomId")
        VALUES ('shared-sha', 'one', $1), ('shared-sha', 'two', $1), ('other-sha', 'three', $1)
        "#,
        bom_id,
    )
    .execute(&pool)
    .await?;

    let result = repo.purge_deleted_project_tree(ROOT_ID).await?;
    assert_eq!(result.project_ids.len(), 4);
    assert_eq!(result.documents.len(), 2);
    assert_eq!(result.chat_ids.len(), 2);
    assert_eq!(
        result.bom_shas,
        vec![("other-sha".to_owned(), 1), ("shared-sha".to_owned(), 2)]
    );

    let purged_ids = result
        .project_ids
        .iter()
        .chain(result.documents.iter().map(|(id, _)| id))
        .chain(&result.chat_ids)
        .cloned()
        .collect::<Vec<_>>();
    let remaining_access = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM entity_access WHERE entity_id::text = ANY($1)"#,
        &purged_ids,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(remaining_access, 0);
    let remaining_permissions = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM "ProjectPermission" WHERE "projectId" = ANY($1)"#,
        &result.project_ids,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(remaining_permissions, 0);
    let remaining_operations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_operations WHERE project_id = ANY($1)")
            .bind(&result.project_ids)
            .fetch_one(&pool)
            .await?;
    assert_eq!(remaining_operations, 0);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn purge_rolls_back_all_deletions(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    repo.soft_delete_project(ROOT_ID).await?;

    let mut transaction = pool.begin().await?;
    let result = super::delete::purge_deleted_project_tree(&mut transaction, ROOT_ID).await?;
    assert!(!result.project_ids.is_empty());
    transaction.rollback().await?;

    assert!(repo.get_basic_project(ROOT_ID).await?.is_some());
    let access_count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM entity_access WHERE entity_id::text = $1"#,
        ROOT_ID,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(access_count, 2);
    let permission_count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM "ProjectPermission" WHERE "projectId" = $1"#,
        ROOT_ID,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(permission_count, 1);
    let operation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_operations WHERE project_id = $1")
            .bind(ROOT_ID)
            .fetch_one(&pool)
            .await?;
    assert_eq!(operation_count, 1);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn upload_folder_preserves_tree_metadata_and_compensates(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let file = FolderItem {
        name: "nested.pdf".to_owned(),
        full_name: "nested.pdf".to_owned(),
        file_type: Some(FileType::Pdf),
        relative_path: "/Upload/Nested".to_owned(),
        sha: "upload-sha".to_owned(),
    };
    let root_folder = FileSystemNode::Folder(HashMap::from([
        (
            "Nested".to_owned(),
            FileSystemNode::Folder(HashMap::from([(
                "nested.pdf".to_owned(),
                FileSystemNode::File(file),
            )])),
        ),
        ("Empty".to_owned(), FileSystemNode::Folder(HashMap::new())),
    ]));
    let mut share_permission = SharePermissionV2::new_project_share_permission(None);
    share_permission.link_share = Some(LinkShare::Team);
    let result = repo
        .upload_folder(UploadFolderRepoArgs {
            user_id: MacroUserIdStr::parse_from_str("macro|owner@test.com")?.into_owned(),
            share_permission,
            root_folder,
            root_folder_name: "Upload".to_owned(),
            upload_request_id: "lambda-request-id".to_owned(),
            parent_id: Some(ROOT_ID.to_owned()),
        })
        .await?;

    assert_eq!(result.project_ids.len(), 3);
    assert_eq!(result.documents.len(), 1);
    let FileSystemNodeWithIds::Folder { project_id, .. } = &result.file_system else {
        panic!("root must be a folder");
    };
    let root = sqlx::query!(
        r#"
        SELECT "parentId" AS parent_id, "uploadPending" AS upload_pending,
               "uploadRequestId" AS upload_request_id
        FROM "Project" WHERE id = $1
        "#,
        project_id,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(root.parent_id.as_deref(), Some(ROOT_ID));
    assert!(root.upload_pending);
    assert_eq!(root.upload_request_id.as_deref(), Some("lambda-request-id"));
    assert_eq!(result.documents[0].project_name.as_deref(), Some("Nested"));

    let document_ids = result
        .documents
        .iter()
        .map(|document| document.document_id.clone())
        .collect::<Vec<_>>();
    let created_permissions = sqlx::query_scalar!(
        r#"
        WITH created_permission_ids AS (
            SELECT "sharePermissionId" AS id
            FROM "ProjectPermission"
            WHERE "projectId" = ANY($1)
            UNION
            SELECT "sharePermissionId" AS id
            FROM "DocumentPermission"
            WHERE "documentId" = ANY($2)
        )
        SELECT COUNT(*) AS "count!"
        FROM "SharePermission" permission
        WHERE permission.id IN (SELECT id FROM created_permission_ids)
          AND permission."linkShare" = 'TEAM'
          AND permission."linkShareAccessLevel" = 'view'
        "#,
        &result.project_ids,
        &document_ids,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(created_permissions, 4);

    repo.delete_uploaded_tree(&result.project_ids, &document_ids)
        .await?;
    let remaining = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM "Project" WHERE id = ANY($1)"#,
        &result.project_ids,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(remaining, 0);
    let remaining_access = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM entity_access WHERE entity_id::text = ANY($1)"#,
        &document_ids,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(remaining_access, 0);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn mark_uploaded_is_recursive_and_rejects_missing_root(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let uploaded_tree = repo
        .mark_projects_uploaded("10000000-0000-0000-0000-000000000006")
        .await?;
    assert_eq!(uploaded_tree.id, "10000000-0000-0000-0000-000000000006");
    assert_eq!(uploaded_tree.name, "Owner pending");
    assert_eq!(uploaded_tree.user_id.as_ref(), "macro|owner@test.com");
    assert_eq!(uploaded_tree.parent_id, None);
    assert!(uploaded_tree.upload_pending_transitioned);

    let mut project_ids = uploaded_tree.project_ids;
    project_ids.sort();
    assert_eq!(
        project_ids,
        [
            "10000000-0000-0000-0000-000000000006".to_owned(),
            "10000000-0000-0000-0000-000000000008".to_owned(),
        ]
    );
    let pending = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM "Project" WHERE id = ANY($1) AND "uploadPending""#,
        &project_ids,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(pending, 0);

    let repeated = repo
        .mark_projects_uploaded("10000000-0000-0000-0000-000000000006")
        .await?;
    assert!(!repeated.upload_pending_transitioned);
    let mut repeated_project_ids = repeated.project_ids;
    repeated_project_ids.sort();
    assert_eq!(repeated_project_ids, project_ids);

    let missing_error = repo.mark_projects_uploaded("missing").await.unwrap_err();
    assert!(matches!(missing_error, sqlx::Error::RowNotFound));
    Ok(())
}
