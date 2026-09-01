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
use system_properties::{StatusOption, SystemPropertyKey};

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

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn task_progress_is_scoped_and_aggregates_only_live_direct_canonical_tasks(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let team_id = uuid::Uuid::from_u128(309);
    sqlx::query(r#"INSERT INTO "team" (id, name, owner_id) VALUES ($1, 'task-progress', 'macro|owner@test.com')"#)
        .bind(team_id).execute(&pool).await?;
    sqlx::query("INSERT INTO team_user (user_id, team_id, team_role) VALUES ($1, $2, 'owner')")
        .bind("macro|owner@test.com")
        .bind(team_id)
        .execute(&pool)
        .await?;
    let ids = [
        "301", "302", "303", "304", "305", "306", "307", "308", "309", "310", "311",
    ];
    for (index, suffix) in ids.into_iter().enumerate() {
        let id = format!("20000000-0000-0000-0000-000000000{suffix}");
        let (project_id, deleted) = if index == 7 {
            (CHILD_ID, None)
        } else if index == 8 {
            (ROOT_ID, Some("2026-09-01"))
        } else {
            (ROOT_ID, None)
        };
        sqlx::query(r#"INSERT INTO "Document" (id, name, owner, "projectId", "deletedAt") VALUES ($1, $2, 'macro|owner@test.com', $3, $4::timestamp)"#)
            .bind(&id).bind(format!("task-{index}")).bind(project_id).bind(deleted).execute(&pool).await?;
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task'::document_sub_type_value)")
            .bind(&id).execute(&pool).await?;
        if index < 5 || index == 6 || index > 8 {
            let values = match index {
                0 => {
                    serde_json::json!({"type":"SelectOption","value":[system_properties::StatusOption::COMPLETED_UUID]})
                }
                1 => {
                    serde_json::json!({"type":"SelectOption","value":[system_properties::StatusOption::CANCELED_UUID]})
                }
                2 => serde_json::Value::Null,
                3 => serde_json::json!({"type":"SelectOption","value":[]}),
                4 => {
                    serde_json::json!({"type":"SelectOption","value":[uuid::Uuid::from_u128(999)]})
                }
                9 => {
                    serde_json::json!({"type":"Boolean","value":true})
                }
                10 => {
                    serde_json::json!({"type":"SelectOption","value":[system_properties::StatusOption::COMPLETED_UUID, system_properties::StatusOption::IN_PROGRESS_UUID]})
                }
                _ => {
                    serde_json::json!({"type":"SelectOption","value":[system_properties::StatusOption::IN_PROGRESS_UUID]})
                }
            };
            sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
                .bind(uuid::Uuid::new_v4()).bind(&id).bind(system_properties::SystemPropertyKey::STATUS_UUID).bind(values).execute(&pool).await?;
        }
    }
    sqlx::query(r#"INSERT INTO "Document" (id, name, owner, "projectId") VALUES ('20000000-0000-0000-0000-000000000312', 'non-task', 'macro|owner@test.com', $1)"#)
        .bind(ROOT_ID).execute(&pool).await?;
    let zero_task_project = "10000000-0000-0000-0000-000000000310";
    let departed_owner_project = "10000000-0000-0000-0000-000000000311";
    let deleted_project = "10000000-0000-0000-0000-000000000312";
    for (id, owner, deleted_at) in [
        (zero_task_project, "macro|owner@test.com", None),
        (departed_owner_project, "macro|viewer@test.com", None),
        (deleted_project, "macro|owner@test.com", Some("2026-09-01")),
    ] {
        sqlx::query(r#"INSERT INTO "Project" (id, name, "userId", "deletedAt") VALUES ($1, $2, $3, $4::timestamp)"#)
            .bind(id).bind(format!("task-progress-{id}")).bind(owner).bind(deleted_at).execute(&pool).await?;
    }
    let progress = repo
        .get_project_task_progress_scoped(ROOT_ID, team_id)
        .await?
        .unwrap();
    assert_eq!(
        (
            progress.completed_tasks,
            progress.included_tasks,
            progress.has_unavailable_statuses
        ),
        (1, 8, true)
    );
    assert_eq!(
        repo.get_project_task_progress_scoped(ROOT_ID, uuid::Uuid::from_u128(310))
            .await?,
        None
    );
    assert_eq!(
        repo.get_project_task_progress_scoped(zero_task_project, team_id)
            .await?,
        Some(crate::domain::models::ProjectTaskProgress::new(
            0, 0, false
        )?)
    );
    for id in ["missing", deleted_project, departed_owner_project] {
        assert_eq!(
            repo.get_project_task_progress_scoped(id, team_id).await?,
            None
        );
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn task_risk_is_scoped_fail_closed_and_uses_calendar_date(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let project = "10000000-0000-0000-0000-000000000401";
    let other_project = "10000000-0000-0000-0000-000000000402";
    let zero_project = "10000000-0000-0000-0000-000000000403";
    let deleted_project = "10000000-0000-0000-0000-000000000404";
    let departed_project = "10000000-0000-0000-0000-000000000405";
    let team = uuid::Uuid::from_u128(0x401);
    sqlx::query(
        r#"INSERT INTO "team" (id,name,owner_id) VALUES ($1,'task-risk','macro|owner@test.com')"#,
    )
    .bind(team)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO team_user (user_id,team_id,team_role) VALUES ('macro|owner@test.com',$1,'owner')").bind(team).execute(&pool).await?;
    sqlx::query(
        "INSERT INTO macro_user (id,username,email,stripe_customer_id) VALUES ($1::uuid,$2,$2,$3)",
    )
    .bind("a4444444-4444-4444-4444-444444444444")
    .bind("departed@test.com")
    .bind("stripe-risk-departed")
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO \"User\" (id,email,\"stripeCustomerId\",macro_user_id) VALUES ($1,$2,$3,$4::uuid)")
        .bind("macro|departed@test.com")
        .bind("departed@test.com")
        .bind("stripe-risk-departed")
        .bind("a4444444-4444-4444-4444-444444444444")
        .execute(&pool)
        .await?;
    for (id, owner, deleted) in [
        (project, "macro|owner@test.com", false),
        (other_project, "macro|owner@test.com", false),
        (zero_project, "macro|owner@test.com", false),
        (deleted_project, "macro|owner@test.com", true),
        (departed_project, "macro|departed@test.com", false),
    ] {
        sqlx::query(r#"INSERT INTO "Project" (id,name,"userId","deletedAt") VALUES ($1,$1,$2,CASE WHEN $3 THEN now() ELSE NULL END)"#).bind(id).bind(owner).bind(deleted).execute(&pool).await?;
    }
    let status = SystemPropertyKey::STATUS_UUID;
    let due = SystemPropertyKey::DUE_DATE_UUID;
    let assignees = SystemPropertyKey::ASSIGNEES_UUID;
    let depends = SystemPropertyKey::DEPENDS_ON_UUID;
    // past/equal/future Date, malformed Date, absent/null/malformed source status,
    // empty/nonempty/malformed assignees, and direct-child-only scope are all represented.
    let rows = [
        (
            "40000000-0000-0000-0000-000000000401",
            project,
            serde_json::json!({"type":"Date","value":"2026-08-31T00:00:00Z"}),
            None,
        ),
        (
            "40000000-0000-0000-0000-000000000402",
            project,
            serde_json::json!({"type":"Date","value":"2026-09-01T00:00:00Z"}),
            None,
        ),
        (
            "40000000-0000-0000-0000-000000000403",
            project,
            serde_json::json!({"type":"Date","value":"2026-09-02T00:00:00Z"}),
            None,
        ),
        (
            "40000000-0000-0000-0000-000000000404",
            project,
            serde_json::json!({"type":"Date","value":"2026-02-31T00:00:00Z"}),
            None,
        ),
        (
            "40000000-0000-0000-0000-000000000405",
            project,
            serde_json::json!({"type":"Date","value":"2026-08-30T00:00:00Z"}),
            Some(serde_json::json!({"type":"Boolean","value":true})),
        ),
        (
            "40000000-0000-0000-0000-000000000406",
            project,
            serde_json::json!({"type":"Date","value":"2026-08-30T00:00:00Z"}),
            Some(
                serde_json::json!({"type":"SelectOption","value":[StatusOption::IN_PROGRESS_UUID,StatusOption::IN_REVIEW_UUID]}),
            ),
        ),
        (
            "40000000-0000-0000-0000-000000000407",
            project,
            serde_json::json!({"type":"Date","value":"2026-08-30T00:00:00Z"}),
            Some(serde_json::json!({"type":"SelectOption","value":[uuid::Uuid::new_v4()]})),
        ),
        (
            "40000000-0000-0000-0000-000000000408",
            project,
            serde_json::json!({"type":"Date","value":"2026-08-30T00:00:00Z"}),
            Some(serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]})),
        ),
        (
            "40000000-0000-0000-0000-000000000409",
            project,
            serde_json::json!({"type":"Date","value":"2026-08-30T00:00:00Z"}),
            Some(serde_json::json!({"type":"SelectOption","value":[StatusOption::CANCELED_UUID]})),
        ),
        (
            "40000000-0000-0000-0000-000000000410",
            other_project,
            serde_json::json!({"type":"Date","value":"2026-08-30T00:00:00Z"}),
            None,
        ),
    ];
    for (id, owner_project, due_value, status_value) in rows {
        sqlx::query(r#"INSERT INTO "Document" (id,name,owner,"projectId") VALUES ($1,$1,'macro|owner@test.com',$2)"#).bind(id).bind(owner_project).execute(&pool).await?;
        sqlx::query("INSERT INTO document_sub_type (document_id,sub_type) VALUES ($1,'task'::document_sub_type_value)").bind(id).execute(&pool).await?;
        sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)").bind(uuid::Uuid::new_v4()).bind(id).bind(due).bind(due_value).execute(&pool).await?;
        if let Some(value) = status_value {
            sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)").bind(uuid::Uuid::new_v4()).bind(id).bind(status).bind(value).execute(&pool).await?;
        }
    }
    macro_rules! source_property_case {
        ($id:expr, $status_value:expr, $due_value:expr, $assignee_value:expr) => {{
            sqlx::query(r#"INSERT INTO "Document" (id,name,owner,"projectId") VALUES ($1,$1,'macro|owner@test.com',$2)"#)
                .bind($id)
                .bind(project)
                .execute(&pool)
                .await?;
            sqlx::query("INSERT INTO document_sub_type (document_id,sub_type) VALUES ($1,'task'::document_sub_type_value)")
                .bind($id)
                .execute(&pool)
                .await?;
            if let Some(value) = $status_value {
                sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)")
                    .bind(uuid::Uuid::new_v4()).bind($id).bind(status).bind(value).execute(&pool).await?;
            }
            if let Some(value) = $due_value {
                sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)")
                    .bind(uuid::Uuid::new_v4()).bind($id).bind(due).bind(value).execute(&pool).await?;
            }
            if let Some(value) = $assignee_value {
                sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)")
                    .bind(uuid::Uuid::new_v4()).bind($id).bind(assignees).bind(value).execute(&pool).await?;
            }
        }};
    }
    let valid_assignee = serde_json::json!({"type":"EntityReference","value":[{"entity_type":"USER","entity_id":"macro|a@test.com"}]});
    // Exact open statuses are evaluable.  The first is past; the other two
    // verify equal/future dates are not overdue while their empty/null
    // assignees remain known-unassigned.
    source_property_case!(
        "40000000-0000-0000-0000-000000000412",
        Some(serde_json::json!({"type":"SelectOption","value":[StatusOption::NOT_STARTED_UUID]})),
        Some(serde_json::json!({"type":"Date","value":"2026-08-31T00:00:00Z"})),
        None::<serde_json::Value>
    );
    source_property_case!(
        "40000000-0000-0000-0000-000000000413",
        Some(serde_json::json!({"type":"SelectOption","value":[StatusOption::IN_PROGRESS_UUID]})),
        Some(serde_json::json!({"type":"Date","value":"2026-09-01T00:00:00Z"})),
        Some(serde_json::Value::Null)
    );
    source_property_case!(
        "40000000-0000-0000-0000-000000000414",
        Some(serde_json::json!({"type":"SelectOption","value":[StatusOption::IN_REVIEW_UUID]})),
        Some(serde_json::json!({"type":"Date","value":"2026-09-02T00:00:00Z"})),
        Some(serde_json::json!({"type":"EntityReference","value":[]}))
    );
    // JSON-null status is open and a JSON-null Due Date is known not overdue.
    source_property_case!(
        "40000000-0000-0000-0000-000000000415",
        Some(serde_json::Value::Null),
        Some(serde_json::Value::Null),
        Some(valid_assignee.clone())
    );
    // Empty status and malformed Due/Assignee representations are unavailable
    // conservative non-claims even when another field would otherwise signal risk.
    source_property_case!(
        "40000000-0000-0000-0000-000000000416",
        Some(serde_json::json!({"type":"SelectOption","value":[]})),
        Some(serde_json::json!({"type":"Date","value":"2026-08-31T00:00:00Z"})),
        Some(serde_json::json!({"type":"EntityReference","value":[]}))
    );
    source_property_case!(
        "40000000-0000-0000-0000-000000000417",
        None::<serde_json::Value>,
        Some(serde_json::json!({"type":"Boolean","value":true})),
        Some(valid_assignee.clone())
    );
    source_property_case!(
        "40000000-0000-0000-0000-000000000418",
        None::<serde_json::Value>,
        Some(serde_json::Value::Null),
        Some(serde_json::Value::Null)
    );
    source_property_case!(
        "40000000-0000-0000-0000-000000000419",
        None::<serde_json::Value>,
        None::<serde_json::Value>,
        Some(
            serde_json::json!({"type":"EntityReference","value":[{"entity_type":"USER","entity_id":"macro|a@test.com","specific_message_id":"message"}]})
        )
    );
    source_property_case!(
        "40000000-0000-0000-0000-000000000420",
        None::<serde_json::Value>,
        None::<serde_json::Value>,
        Some(
            serde_json::json!({"type":"EntityReference","value":[{"entity_type":"USER","entity_id":""}]})
        )
    );
    source_property_case!(
        "40000000-0000-0000-0000-000000000421",
        None::<serde_json::Value>,
        None::<serde_json::Value>,
        Some(
            serde_json::json!({"type":"EntityReference","value":[{"entity_type":"USER","entity_id":"macro|a@test.com"},{"entity_type":"TASK","entity_id":"x"}]})
        )
    );
    // Fractional UTC seconds remain canonical Date values; the date component
    // is still a strict calendar-date comparison, not a timestamp policy.
    source_property_case!(
        "40000000-0000-0000-0000-000000000422",
        None::<serde_json::Value>,
        Some(serde_json::json!({"type":"Date","value":"2026-08-31T23:59:59.123456789Z"})),
        Some(valid_assignee.clone())
    );
    for (id, value) in [
        (
            "40000000-0000-0000-0000-000000000401",
            serde_json::json!({"type":"EntityReference","value":[]}),
        ),
        (
            "40000000-0000-0000-0000-000000000402",
            serde_json::json!({"type":"EntityReference","value":[{"entity_type":"USER","entity_id":"macro|a@test.com"}]}),
        ),
        (
            "40000000-0000-0000-0000-000000000403",
            serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":"x"}]}),
        ),
    ] {
        sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)").bind(uuid::Uuid::new_v4()).bind(id).bind(assignees).bind(value).execute(&pool).await?;
    }
    // A missing entity_type is malformed: it is unavailable and must not be
    // converted into an unassigned aggregate claim.
    sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)")
        .bind(uuid::Uuid::new_v4())
        .bind("40000000-0000-0000-0000-000000000404")
        .bind(assignees)
        .bind(serde_json::json!({"type":"EntityReference","value":[{"entity_id":"macro|missing-type@test.com"}]}))
        .execute(&pool)
        .await?;
    let predecessor = "40000000-0000-0000-0000-000000000411";
    sqlx::query(r#"INSERT INTO "Document" (id,name,owner,"projectId") VALUES ($1,$1,'macro|owner@test.com',$2)"#).bind(predecessor).bind(project).execute(&pool).await?;
    sqlx::query("INSERT INTO document_sub_type (document_id,sub_type) VALUES ($1,'task'::document_sub_type_value)").bind(predecessor).execute(&pool).await?;
    sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)").bind(uuid::Uuid::new_v4()).bind(predecessor).bind(status).bind(serde_json::json!({"type":"SelectOption","value":[StatusOption::IN_PROGRESS_UUID]})).execute(&pool).await?;
    sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)").bind(uuid::Uuid::new_v4()).bind("40000000-0000-0000-0000-000000000401").bind(depends).bind(serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":predecessor}]})).execute(&pool).await?;
    // Null and exact-empty dependencies are both available and ready; they
    // must not acquire a blocking null row from the lateral expansion.
    for (id, value) in [
        (
            "40000000-0000-0000-0000-000000000402",
            serde_json::Value::Null,
        ),
        (
            "40000000-0000-0000-0000-000000000403",
            serde_json::json!({"type":"EntityReference","value":[]}),
        ),
    ] {
        sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)").bind(uuid::Uuid::new_v4()).bind(id).bind(depends).bind(value).execute(&pool).await?;
    }
    // These would each add overdue + unassigned if the aggregate accidentally
    // traversed descendants, accepted non-Tasks, or included deleted Tasks.
    let child_project = "10000000-0000-0000-0000-000000000406";
    sqlx::query(r#"INSERT INTO "Project" (id,name,"userId","parentId") VALUES ($1,'child-risk','macro|owner@test.com',$2)"#)
        .bind(child_project)
        .bind(project)
        .execute(&pool)
        .await?;
    for (id, task_project, deleted, is_task) in [
        ("40000000-0000-0000-0000-000000000430", project, true, true),
        (
            "40000000-0000-0000-0000-000000000431",
            project,
            false,
            false,
        ),
        (
            "40000000-0000-0000-0000-000000000432",
            child_project,
            false,
            true,
        ),
    ] {
        sqlx::query(r#"INSERT INTO "Document" (id,name,owner,"projectId","deletedAt") VALUES ($1,$1,'macro|owner@test.com',$2,CASE WHEN $3 THEN now() ELSE NULL END)"#)
            .bind(id).bind(task_project).bind(deleted).execute(&pool).await?;
        if is_task {
            sqlx::query("INSERT INTO document_sub_type (document_id,sub_type) VALUES ($1,'task'::document_sub_type_value)").bind(id).execute(&pool).await?;
        }
        for (key, value) in [
            (
                status,
                serde_json::json!({"type":"SelectOption","value":[StatusOption::IN_PROGRESS_UUID]}),
            ),
            (
                due,
                serde_json::json!({"type":"Date","value":"2026-08-31T00:00:00Z"}),
            ),
            (
                assignees,
                serde_json::json!({"type":"EntityReference","value":[]}),
            ),
        ] {
            sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)")
                .bind(uuid::Uuid::new_v4()).bind(id).bind(key).bind(value).execute(&pool).await?;
        }
    }
    let risk = PgProjectRepo::new(pool.clone())
        .get_project_task_risk_scoped(
            project,
            team,
            chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
        )
        .await?
        .unwrap();
    assert_eq!(
        (
            risk.overdue_tasks,
            risk.blocked_tasks,
            risk.unassigned_tasks
        ),
        // baseline direct cases contribute (1, 1, 2); canonical/null source
        // cases add (+1, 0, +4), and fractional UTC adds (+1, 0, 0).
        (3, 1, 6)
    );
    assert!(risk.has_unavailable_risk_data);
    for unavailable in ["missing", deleted_project, departed_project] {
        assert!(
            PgProjectRepo::new(pool.clone())
                .get_project_task_risk_scoped(
                    unavailable,
                    team,
                    chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()
                )
                .await?
                .is_none()
        );
    }
    assert_eq!(
        PgProjectRepo::new(pool.clone())
            .get_project_task_risk_scoped(
                zero_project,
                team,
                chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()
            )
            .await?,
        Some(crate::domain::models::ProjectTaskRisk::new(
            0, 0, 0, false, true
        )?)
    );
    assert!(
        PgProjectRepo::new(pool)
            .get_project_task_risk_scoped(
                project,
                uuid::Uuid::new_v4(),
                chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()
            )
            .await?
            .is_none()
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn task_risk_includes_only_operational_targets_approaching_the_caller_date(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let team = uuid::Uuid::from_u128(0x451);
    let project = "10000000-0000-0000-0000-000000000451";
    let zero_project = "10000000-0000-0000-0000-000000000452";
    let missing_operations_project = "10000000-0000-0000-0000-000000000453";
    let as_of_date = chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
    let repo = PgProjectRepo::new(pool.clone());

    sqlx::query(
        r#"INSERT INTO "team" (id,name,owner_id) VALUES ($1,'target-risk','macro|owner@test.com')"#,
    )
    .bind(team)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO team_user (user_id,team_id,team_role) VALUES ('macro|owner@test.com',$1,'owner')")
        .bind(team)
        .execute(&pool)
        .await?;
    for id in [project, zero_project, missing_operations_project] {
        sqlx::query(
            r#"INSERT INTO "Project" (id,name,"userId") VALUES ($1,$1,'macro|owner@test.com')"#,
        )
        .bind(id)
        .execute(&pool)
        .await?;
    }

    macro_rules! assert_risk {
        ($id:expr, $approaching:expr, $unavailable:expr) => {{
            let risk = repo
                .get_project_task_risk_scoped($id, team, as_of_date)
                .await?
                .expect("scoped project risk");
            assert_eq!(
                (
                    risk.overdue_tasks,
                    risk.blocked_tasks,
                    risk.unassigned_tasks,
                    risk.approaching_target,
                    risk.has_unavailable_risk_data,
                ),
                (0, 0, 0, $approaching, $unavailable)
            );
        }};
    }

    for (status, target_date, approaching) in [
        ("planned", as_of_date, true),
        ("active", as_of_date + chrono::Days::new(7), true),
        ("active", as_of_date - chrono::Days::new(1), false),
        ("active", as_of_date + chrono::Days::new(8), false),
        ("paused", as_of_date, false),
        ("completed", as_of_date, false),
        ("archived", as_of_date, false),
    ] {
        sqlx::query(
            "UPDATE project_operations SET status = $1, target_date = $2 WHERE project_id = $3",
        )
        .bind(status)
        .bind(target_date)
        .bind(project)
        .execute(&pool)
        .await?;
        assert_risk!(project, approaching, false);
    }

    sqlx::query("UPDATE project_operations SET status = 'planned', target_date = NULL WHERE project_id = $1")
        .bind(project)
        .execute(&pool)
        .await?;
    assert_risk!(project, false, true);

    sqlx::query(
        "UPDATE project_operations SET status = 'active', target_date = $1 WHERE project_id = $2",
    )
    .bind(as_of_date)
    .bind(zero_project)
    .execute(&pool)
    .await?;
    assert_risk!(zero_project, true, false);

    sqlx::query("DELETE FROM project_operations WHERE project_id = $1")
        .bind(missing_operations_project)
        .execute(&pool)
        .await?;
    assert_risk!(missing_operations_project, false, true);
    Ok(())
}

/// Dependency cases deliberately use an absent Due Date and one valid assignee:
/// the aggregate can therefore prove readiness semantics without a second risk
/// dimension changing the count.
#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn task_risk_dependency_matrix_preserves_ready_blocked_and_unavailable_directions(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let project = "10000000-0000-0000-0000-000000000451";
    let other_project = "10000000-0000-0000-0000-000000000452";
    let team = uuid::Uuid::from_u128(0x451);
    let status = SystemPropertyKey::STATUS_UUID;
    let depends = SystemPropertyKey::DEPENDS_ON_UUID;
    let assignees = SystemPropertyKey::ASSIGNEES_UUID;
    sqlx::query(r#"INSERT INTO "team" (id,name,owner_id) VALUES ($1,'risk-dependency','macro|owner@test.com')"#)
        .bind(team).execute(&pool).await?;
    sqlx::query("INSERT INTO team_user (user_id,team_id,team_role) VALUES ('macro|owner@test.com',$1,'owner')")
        .bind(team).execute(&pool).await?;
    for id in [project, other_project] {
        sqlx::query(
            r#"INSERT INTO "Project" (id,name,"userId") VALUES ($1,$1,'macro|owner@test.com')"#,
        )
        .bind(id)
        .execute(&pool)
        .await?;
    }
    macro_rules! task {
        ($id:expr, $task_project:expr) => {{
            sqlx::query(r#"INSERT INTO "Document" (id,name,owner,"projectId") VALUES ($1,$1,'macro|owner@test.com',$2)"#)
                .bind($id).bind($task_project).execute(&pool).await?;
            sqlx::query("INSERT INTO document_sub_type (document_id,sub_type) VALUES ($1,'task'::document_sub_type_value)")
                .bind($id).execute(&pool).await?;
            sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)")
                .bind(uuid::Uuid::new_v4()).bind($id).bind(assignees)
                .bind(serde_json::json!({"type":"EntityReference","value":[{"entity_type":"USER","entity_id":"macro|a@test.com"}]}))
                .execute(&pool).await?;
        }};
    }
    macro_rules! property {
        ($id:expr, $key:expr, $value:expr) => {{
            sqlx::query("INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4)")
                .bind(uuid::Uuid::new_v4()).bind($id).bind($key).bind($value).execute(&pool).await?;
        }};
    }
    let completed = "45000000-0000-0000-0000-000000000401";
    let in_progress = "45000000-0000-0000-0000-000000000402";
    let canceled = "45000000-0000-0000-0000-000000000403";
    let malformed_status = "45000000-0000-0000-0000-000000000404";
    let unknown_status = "45000000-0000-0000-0000-000000000405";
    for id in [
        completed,
        in_progress,
        canceled,
        malformed_status,
        unknown_status,
    ] {
        task!(id, project);
    }
    property!(
        completed,
        status,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]})
    );
    property!(
        in_progress,
        status,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::IN_PROGRESS_UUID]})
    );
    property!(
        canceled,
        status,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::CANCELED_UUID]})
    );
    property!(
        malformed_status,
        status,
        serde_json::json!({"type":"Boolean","value":true})
    );
    property!(
        unknown_status,
        status,
        serde_json::json!({"type":"SelectOption","value":[uuid::Uuid::new_v4()]})
    );
    // ready: absent, JSON null, exact empty, and exact Completed predecessor.
    let ready_absent = "45000000-0000-0000-0000-000000000411";
    let ready_null = "45000000-0000-0000-0000-000000000412";
    let ready_empty = "45000000-0000-0000-0000-000000000413";
    let ready_completed = "45000000-0000-0000-0000-000000000414";
    // available blocked: exact nonterminal and exact Canceled predecessor.
    let blocked_open = "45000000-0000-0000-0000-000000000415";
    let blocked_canceled = "45000000-0000-0000-0000-000000000416";
    // unavailable blocked: malformed container/ref/self/missing/cross-project/nonTask.
    // Live malformed/unknown predecessor statuses remain available-but-blocked,
    // matching the established WS-04 dependency readiness query.
    let unavailable = [
        "45000000-0000-0000-0000-000000000421",
        "45000000-0000-0000-0000-000000000422",
        "45000000-0000-0000-0000-000000000423",
        "45000000-0000-0000-0000-000000000424",
        "45000000-0000-0000-0000-000000000425",
        "45000000-0000-0000-0000-000000000426",
        "45000000-0000-0000-0000-000000000427",
        "45000000-0000-0000-0000-000000000428",
        "45000000-0000-0000-0000-000000000429",
    ];
    for id in [
        ready_absent,
        ready_null,
        ready_empty,
        ready_completed,
        blocked_open,
        blocked_canceled,
    ]
    .into_iter()
    .chain(unavailable)
    {
        task!(id, project);
    }
    property!(ready_null, depends, serde_json::Value::Null);
    property!(
        ready_empty,
        depends,
        serde_json::json!({"type":"EntityReference","value":[]})
    );
    property!(
        ready_completed,
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":completed}]})
    );
    property!(
        blocked_open,
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":in_progress}]})
    );
    property!(
        blocked_canceled,
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":canceled}]})
    );
    property!(
        unavailable[0],
        depends,
        serde_json::json!({"type":"Boolean","value":true})
    );
    property!(
        unavailable[1],
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_id":"45000000-0000-0000-0000-000000000401"}]})
    );
    property!(
        unavailable[2],
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":unavailable[2]}]})
    );
    property!(
        unavailable[3],
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":"45000000-0000-0000-0000-000000000499"}]})
    );
    task!("45000000-0000-0000-0000-000000000498", other_project);
    property!(
        unavailable[4],
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":"45000000-0000-0000-0000-000000000498"}]})
    );
    sqlx::query(r#"INSERT INTO "Document" (id,name,owner,"projectId") VALUES ('45000000-0000-0000-0000-000000000497','non-task','macro|owner@test.com',$1)"#).bind(project).execute(&pool).await?;
    property!(
        unavailable[5],
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":"45000000-0000-0000-0000-000000000497"}]})
    );
    property!(
        unavailable[6],
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":malformed_status}]})
    );
    property!(
        unavailable[7],
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":unknown_status}]})
    );
    // A deleted same-project Task remains an unavailable predecessor even if
    // its stored status says Completed: live identity is the first boundary.
    sqlx::query(r#"INSERT INTO "Document" (id,name,owner,"projectId","deletedAt") VALUES ('45000000-0000-0000-0000-000000000496','deleted-task','macro|owner@test.com',$1,now())"#)
        .bind(project)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO document_sub_type (document_id,sub_type) VALUES ('45000000-0000-0000-0000-000000000496','task'::document_sub_type_value)")
        .execute(&pool)
        .await?;
    property!(
        "45000000-0000-0000-0000-000000000496",
        status,
        serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]})
    );
    property!(
        unavailable[8],
        depends,
        serde_json::json!({"type":"EntityReference","value":[{"entity_type":"TASK","entity_id":"45000000-0000-0000-0000-000000000496"}]})
    );
    let risk = PgProjectRepo::new(pool)
        .get_project_task_risk_scoped(
            project,
            team,
            chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
        )
        .await?
        .unwrap();
    // 2 valid blocking predecessors + 9 fail-closed dependency cases; four ready cases contribute none.
    assert_eq!(
        (
            risk.overdue_tasks,
            risk.blocked_tasks,
            risk.unassigned_tasks
        ),
        (0, 11, 0)
    );
    assert!(risk.has_unavailable_risk_data);
    Ok(())
}

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
async fn subtree_lifecycle_serializes_behind_taskhier_lock(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let mut delete_lock = pool.begin().await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKHIER")
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
    delete.await??;
    let mut restore_lock = pool.begin().await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKHIER")
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
    restore.await??;
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn subtree_lifecycle_preserves_task_hierarchy_and_document_projects(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let parent = "20000000-0000-0000-0000-000000000102";
    let child = "20000000-0000-0000-0000-000000000107";
    let parent_value = serde_json::json!({"type":"EntityReference","value":[{"entity_id":child,"entity_type":"TASK","specific_message_id":null}]});
    let child_value = serde_json::json!({"type":"EntityReference","value":[{"entity_id":parent,"entity_type":"TASK","specific_message_id":null}]});
    for (entity_id, definition, value) in [
        (
            parent,
            system_properties::SystemPropertyKey::SUBTASKS_UUID,
            parent_value.clone(),
        ),
        (
            child,
            system_properties::SystemPropertyKey::PARENT_TASK_UUID,
            child_value.clone(),
        ),
    ] {
        sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
            .bind(uuid::Uuid::new_v4()).bind(entity_id).bind(definition).bind(value).execute(&pool).await?;
    }
    let before_projects: Vec<Option<String>> =
        sqlx::query_scalar("SELECT \"projectId\" FROM \"Document\" WHERE id = ANY($1) ORDER BY id")
            .bind(vec![parent, child])
            .fetch_all(&pool)
            .await?;
    repo.soft_delete_project(ROOT_ID).await?;
    repo.revert_delete_project(ROOT_ID, None).await?;
    let after_projects: Vec<Option<String>> =
        sqlx::query_scalar("SELECT \"projectId\" FROM \"Document\" WHERE id = ANY($1) ORDER BY id")
            .bind(vec![parent, child])
            .fetch_all(&pool)
            .await?;
    assert_eq!(after_projects, before_projects);
    let stored_parent: serde_json::Value = sqlx::query_scalar(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2",
    )
    .bind(parent)
    .bind(system_properties::SystemPropertyKey::SUBTASKS_UUID)
    .fetch_one(&pool)
    .await?;
    let stored_child: serde_json::Value = sqlx::query_scalar(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2",
    )
    .bind(child)
    .bind(system_properties::SystemPropertyKey::PARENT_TASK_UUID)
    .fetch_one(&pool)
    .await?;
    assert_eq!(stored_parent, parent_value);
    assert_eq!(stored_child, child_value);
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

    let deleted_at = repo
        .get_basic_project(ROOT_ID)
        .await?
        .and_then(|project| project.deleted_at)
        .expect("soft-delete token");
    let purged = repo
        .purge_deleted_project_tree_if_token(ROOT_ID, deleted_at)
        .await?
        .expect("matching project tree");
    assert_eq!(purged.root.id, ROOT_ID);
    assert_eq!(purged.root.user_id.as_ref(), "macro|owner@test.com");
    assert_eq!(purged.root.parent_id, None);
    assert_eq!(purged.root.deleted_at, Some(deleted_at));
    let result = purged.tree;
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
async fn project_purge_removes_task_sources_and_repairs_within_and_cross_tree_hierarchy(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let source = "20000000-0000-0000-0000-000000000102";
    let peer = "20000000-0000-0000-0000-000000000107";
    let external = "20000000-0000-0000-0000-000000000108";
    for (id, project_id) in [
        (source, ROOT_ID),
        (peer, ROOT_ID),
        (external, "10000000-0000-0000-0000-000000000005"),
    ] {
        sqlx::query("INSERT INTO \"Document\" (id, name, owner, \"fileType\", \"projectId\") VALUES ($1, $2, 'macro|owner@test.com', 'docx', $3)")
            .bind(id)
            .bind(format!("task-{id}"))
            .bind(project_id)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
            .bind(id)
            .execute(&pool)
            .await?;
    }
    let source_edge =
        serde_json::json!({"entity_id":source,"entity_type":"TASK","specific_message_id":"drop"});
    let keep =
        serde_json::json!({"entity_id":external,"entity_type":"TASK","specific_message_id":"keep"});
    for (entity_id, definition, value) in [
        (
            source,
            system_properties::SystemPropertyKey::STATUS_UUID,
            serde_json::json!({"type":"SelectOption","value":[]}),
        ),
        (
            peer,
            system_properties::SystemPropertyKey::SUBTASKS_UUID,
            serde_json::json!({"type":"EntityReference","value":[source_edge.clone(),keep.clone()]}),
        ),
        (
            external,
            system_properties::SystemPropertyKey::PARENT_TASK_UUID,
            serde_json::json!({"type":"EntityReference","value":[source_edge]}),
        ),
    ] {
        sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
            .bind(uuid::Uuid::new_v4())
            .bind(entity_id)
            .bind(definition)
            .bind(value)
            .execute(&pool)
            .await?;
    }

    repo.soft_delete_project(ROOT_ID).await?;
    let token = repo
        .get_basic_project(ROOT_ID)
        .await?
        .and_then(|project| project.deleted_at)
        .expect("soft-delete token");
    assert!(
        repo.purge_deleted_project_tree_if_token(ROOT_ID, token)
            .await?
            .is_some()
    );

    let source_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK'",
    )
    .bind(source)
    .fetch_one(&pool)
    .await?;
    assert_eq!(source_rows, 0);
    let cross_tree: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2",
    )
    .bind(external)
    .bind(system_properties::SystemPropertyKey::PARENT_TASK_UUID)
    .fetch_one(&pool)
    .await?;
    assert_eq!(cross_tree, None);
    let within_tree: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2",
    )
    .bind(peer)
    .bind(system_properties::SystemPropertyKey::SUBTASKS_UUID)
    .fetch_optional(&pool)
    .await?;
    assert_eq!(
        within_tree, None,
        "within-tree task rows are removed with their source"
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn purge_rolls_back_all_deletions(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let source = "20000000-0000-0000-0000-000000000110";
    let survivor = "20000000-0000-0000-0000-000000000111";
    for (id, project_id) in [
        (source, ROOT_ID),
        (survivor, "10000000-0000-0000-0000-000000000005"),
    ] {
        sqlx::query("INSERT INTO \"Document\" (id, name, owner, \"fileType\", \"projectId\") VALUES ($1, $2, 'macro|owner@test.com', 'docx', $3)")
            .bind(id)
            .bind(format!("rollback-task-{id}"))
            .bind(project_id)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
            .bind(id)
            .execute(&pool)
            .await?;
    }
    let reverse = serde_json::json!({"type":"EntityReference","value":[{"entity_id":source,"entity_type":"TASK","extra":"rollback"}]});
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
        .bind(uuid::Uuid::new_v4())
        .bind(source)
        .bind(system_properties::SystemPropertyKey::STATUS_UUID)
        .bind(serde_json::json!({"type":"SelectOption","value":[]}))
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
        .bind(uuid::Uuid::new_v4())
        .bind(survivor)
        .bind(system_properties::SystemPropertyKey::SUBTASKS_UUID)
        .bind(reverse.clone())
        .execute(&pool)
        .await?;
    repo.soft_delete_project(ROOT_ID).await?;
    let deleted_at = repo
        .get_basic_project(ROOT_ID)
        .await?
        .and_then(|project| project.deleted_at)
        .expect("soft-delete token");

    let mut transaction = pool.begin().await?;
    let result =
        super::delete::purge_deleted_project_tree_if_token(&mut transaction, ROOT_ID, deleted_at)
            .await?;
    assert!(result.is_some());
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
    let restored_reverse: serde_json::Value = sqlx::query_scalar(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2",
    )
    .bind(survivor)
    .bind(system_properties::SystemPropertyKey::SUBTASKS_UUID)
    .fetch_one(&pool)
    .await?;
    assert_eq!(restored_reverse, reverse);
    let restored_source_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK'",
    )
    .bind(source)
    .fetch_one(&pool)
    .await?;
    assert_eq!(restored_source_rows, 1);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn purge_exact_token_rejects_stale_missing_and_live_subtree_rows(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    let source = "20000000-0000-0000-0000-000000000112";
    let survivor = "20000000-0000-0000-0000-000000000113";
    for (id, project_id) in [
        (source, ROOT_ID),
        (survivor, "10000000-0000-0000-0000-000000000005"),
    ] {
        sqlx::query("INSERT INTO \"Document\" (id, name, owner, \"fileType\", \"projectId\") VALUES ($1, $2, 'macro|owner@test.com', 'docx', $3)")
            .bind(id)
            .bind(format!("stale-task-{id}"))
            .bind(project_id)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
            .bind(id)
            .execute(&pool)
            .await?;
    }
    let reverse = serde_json::json!({"type":"EntityReference","value":[{"entity_id":source,"entity_type":"TASK","extra":"stale"}]});
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
        .bind(uuid::Uuid::new_v4())
        .bind(source)
        .bind(system_properties::SystemPropertyKey::STATUS_UUID)
        .bind(serde_json::json!({"type":"SelectOption","value":[]}))
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
        .bind(uuid::Uuid::new_v4())
        .bind(survivor)
        .bind(system_properties::SystemPropertyKey::PARENT_TASK_UUID)
        .bind(reverse.clone())
        .execute(&pool)
        .await?;
    repo.soft_delete_project(ROOT_ID).await?;
    let token = repo
        .get_basic_project(ROOT_ID)
        .await?
        .and_then(|project| project.deleted_at)
        .expect("soft-delete token");
    assert!(
        repo.purge_deleted_project_tree_if_token(ROOT_ID, token + chrono::Duration::seconds(1))
            .await?
            .is_none()
    );
    assert!(
        repo.purge_deleted_project_tree_if_token("missing", token)
            .await?
            .is_none()
    );

    for (table, id) in [
        ("Project", CHILD_ID),
        ("Document", "20000000-0000-0000-0000-000000000001"),
        ("Chat", "30000000-0000-0000-0000-000000000001"),
    ] {
        let sql = format!(r#"UPDATE "{table}" SET "deletedAt" = NULL WHERE id = $1"#);
        sqlx::query(&sql).bind(id).execute(&pool).await?;
        assert!(
            repo.purge_deleted_project_tree_if_token(ROOT_ID, token)
                .await?
                .is_none()
        );
        let still_present: bool = sqlx::query_scalar(&format!(
            r#"SELECT EXISTS(SELECT 1 FROM "{table}" WHERE id = $1)"#
        ))
        .bind(id)
        .fetch_one(&pool)
        .await?;
        assert!(still_present);
        let sql = format!(r#"UPDATE "{table}" SET "deletedAt" = $2 WHERE id = $1"#);
        sqlx::query(&sql)
            .bind(id)
            .bind(token.naive_utc())
            .execute(&pool)
            .await?;
    }
    let unchanged_reverse: serde_json::Value = sqlx::query_scalar(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2",
    )
    .bind(survivor)
    .bind(system_properties::SystemPropertyKey::PARENT_TASK_UUID)
    .fetch_one(&pool)
    .await?;
    assert_eq!(unchanged_reverse, reverse);
    let unchanged_source_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK'",
    )
    .bind(source)
    .fetch_one(&pool)
    .await?;
    assert_eq!(unchanged_source_rows, 1);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn purge_rejects_live_root_without_deleting_source_rows(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    repo.soft_delete_project(ROOT_ID).await?;
    let token = repo
        .get_basic_project(ROOT_ID)
        .await?
        .and_then(|project| project.deleted_at)
        .expect("soft-delete token");
    sqlx::query!(
        r#"UPDATE "Project" SET "deletedAt" = NULL WHERE id = $1"#,
        ROOT_ID
    )
    .execute(&pool)
    .await?;
    assert!(
        repo.purge_deleted_project_tree_if_token(ROOT_ID, token)
            .await?
            .is_none()
    );
    assert!(repo.get_basic_project(ROOT_ID).await?.is_some());
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn project_purge_serializes_behind_taskhier_lock(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool.clone());
    repo.soft_delete_project(ROOT_ID).await?;
    let token = repo
        .get_basic_project(ROOT_ID)
        .await?
        .and_then(|project| project.deleted_at)
        .expect("soft-delete token");
    let mut hierarchy_lock = pool.begin().await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKHIER")
    )
    .fetch_one(&mut *hierarchy_lock)
    .await?;
    let purge_repo = repo.clone();
    let mut purge = tokio::spawn(async move {
        purge_repo
            .purge_deleted_project_tree_if_token(ROOT_ID, token)
            .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(25), &mut purge)
            .await
            .is_err()
    );
    hierarchy_lock.commit().await?;
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), purge)
            .await???
            .is_some()
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn taskdeps_restore_race_leaves_one_complete_tree_state(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool);
    let deleted = repo.soft_delete_project(ROOT_ID).await?;
    let token = repo
        .get_basic_project(ROOT_ID)
        .await?
        .and_then(|project| project.deleted_at)
        .expect("soft-delete token");
    let purge_repo = repo.clone();
    let restore_repo = repo.clone();
    let (purge, restore) = tokio::join!(
        async move {
            purge_repo
                .purge_deleted_project_tree_if_token(ROOT_ID, token)
                .await
        },
        async move { restore_repo.revert_delete_project(ROOT_ID, None).await },
    );
    let purge = purge?;
    let restore = restore?;
    assert!(purge.is_some() ^ !restore.project_ids.is_empty());
    let root = repo.get_basic_project(ROOT_ID).await?;
    let project_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Project" WHERE id = ANY($1)"#)
            .bind(&deleted.project_ids)
            .fetch_one(&repo.pool)
            .await?;
    let document_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Document" WHERE id = ANY($1)"#)
            .bind(&deleted.document_ids)
            .fetch_one(&repo.pool)
            .await?;
    let chat_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Chat" WHERE id = ANY($1)"#)
        .bind(&deleted.chat_ids)
        .fetch_one(&repo.pool)
        .await?;
    if purge.is_some() {
        assert!(root.is_none());
        assert_eq!(project_count, 0);
        assert_eq!(document_count, 0);
        assert_eq!(chat_count, 0);
    } else {
        assert!(root.is_some_and(|project| project.deleted_at.is_none()));
        assert_eq!(project_count as usize, deleted.project_ids.len());
        assert_eq!(document_count as usize, deleted.document_ids.len());
        assert_eq!(chat_count as usize, deleted.chat_ids.len());
        let live_projects: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Project" WHERE id = ANY($1) AND "deletedAt" IS NULL"#,
        )
        .bind(&deleted.project_ids)
        .fetch_one(&repo.pool)
        .await?;
        let live_documents: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Document" WHERE id = ANY($1) AND "deletedAt" IS NULL"#,
        )
        .bind(&deleted.document_ids)
        .fetch_one(&repo.pool)
        .await?;
        let live_chats: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Chat" WHERE id = ANY($1) AND "deletedAt" IS NULL"#,
        )
        .bind(&deleted.chat_ids)
        .fetch_one(&repo.pool)
        .await?;
        assert_eq!(live_projects as usize, deleted.project_ids.len());
        assert_eq!(live_documents as usize, deleted.document_ids.len());
        assert_eq!(live_chats as usize, deleted.chat_ids.len());
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("projects_test_data"))
)]
async fn purge_redelete_rejects_old_token_and_successful_retry_is_stale(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgProjectRepo::new(pool);
    repo.soft_delete_project(ROOT_ID).await?;
    let old_token = repo
        .get_basic_project(ROOT_ID)
        .await?
        .and_then(|project| project.deleted_at)
        .expect("old token");
    repo.revert_delete_project(ROOT_ID, None).await?;
    repo.soft_delete_project(ROOT_ID).await?;
    let fresh_token = repo
        .get_basic_project(ROOT_ID)
        .await?
        .and_then(|project| project.deleted_at)
        .expect("fresh token");
    assert!(
        repo.purge_deleted_project_tree_if_token(ROOT_ID, old_token)
            .await?
            .is_none()
    );
    assert!(
        repo.purge_deleted_project_tree_if_token(ROOT_ID, fresh_token)
            .await?
            .is_some()
    );
    assert!(
        repo.purge_deleted_project_tree_if_token(ROOT_ID, fresh_token)
            .await?
            .is_none()
    );
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
