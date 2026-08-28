use super::transition_task_status;
use crate::domain::model::{TaskDependencyReadiness, TaskReadiness, TaskStatusMutationOutcome};
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use models_properties::{EntityReference, EntityType};
use sqlx::{Pool, Postgres};
use system_properties::{StatusOption, SystemPropertyKey};
use uuid::Uuid;

async fn task(pool: &Pool<Postgres>, id: Uuid, project: Option<&str>, is_task: bool) {
    sqlx::query("INSERT INTO \"Document\" (id, name, owner, \"projectId\") VALUES ($1, 'task', 'task-dependencies-owner', $2)").bind(id.to_string()).bind(project).execute(pool).await.unwrap();
    if is_task {
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
            .bind(id.to_string())
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn raw(
    pool: &Pool<Postgres>,
    task: Uuid,
    definition: Uuid,
    value: Option<serde_json::Value>,
) {
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4) ON CONFLICT (entity_id, entity_type, property_definition_id) DO UPDATE SET values = EXCLUDED.values")
        .bind(Uuid::new_v4()).bind(task.to_string()).bind(definition).bind(value).execute(pool).await.unwrap();
}
async fn status(pool: &Pool<Postgres>, task: Uuid, status: Option<StatusOption>) {
    raw(
        pool,
        task,
        SystemPropertyKey::STATUS_UUID,
        status.map(|s| serde_json::json!({"type":"SelectOption","value":[s.uuid()]})),
    )
    .await;
}
async fn status_row(pool: &Pool<Postgres>, task: Uuid) -> (i64, Option<serde_json::Value>) {
    let count = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK' AND property_definition_id = $2",
    )
    .bind(task.to_string())
    .bind(SystemPropertyKey::STATUS_UUID)
    .fetch_one(pool)
    .await
    .unwrap();
    let value = sqlx::query_scalar(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK' AND property_definition_id = $2",
    )
    .bind(task.to_string())
    .bind(SystemPropertyKey::STATUS_UUID)
    .fetch_optional(pool)
    .await
    .unwrap()
    .flatten();
    (count, value)
}
async fn depends(pool: &Pool<Postgres>, task: Uuid, refs: serde_json::Value) {
    raw(pool, task, SystemPropertyKey::DEPENDS_ON_UUID, Some(refs)).await
}

fn blocked_readiness(outcome: TaskStatusMutationOutcome) -> TaskDependencyReadiness {
    let TaskStatusMutationOutcome::BlockedWithReadiness(readiness) = outcome else {
        panic!("expected structured blocked outcome");
    };
    readiness
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn guarded_status_accepts_empty_live_task(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    task(&pool, source, None, true).await;
    for status in [
        StatusOption::InProgress,
        StatusOption::InReview,
        StatusOption::Completed,
    ] {
        assert!(matches!(
            transition_task_status(&pool, source, Some(status)).await?,
            TaskStatusMutationOutcome::Updated(_)
        ));
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn missing_deleted_or_non_task_source_blocks_unguarded_transitions_without_status_rows(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let missing = Uuid::new_v4();
    let deleted = Uuid::new_v4();
    let non_task = Uuid::new_v4();
    task(&pool, deleted, None, true).await;
    task(&pool, non_task, None, false).await;
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\" = NOW() WHERE id = $1")
        .bind(deleted.to_string())
        .execute(&pool)
        .await?;
    for source in [missing, deleted, non_task] {
        for target in [
            Some(StatusOption::NotStarted),
            Some(StatusOption::Canceled),
            None,
        ] {
            assert!(matches!(
                transition_task_status(&pool, source, target).await?,
                TaskStatusMutationOutcome::Blocked
            ));
            assert_eq!(status_row(&pool, source).await, (0, None));
        }
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn guarded_status_requires_exact_completed_live_predecessors(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let predecessor = Uuid::new_v4();
    task(&pool, source, None, true).await;
    task(&pool, predecessor, None, true).await;
    depends(&pool,source,serde_json::json!({"type":"EntityReference","value":[EntityReference::new(predecessor.to_string(),EntityType::Task)]})).await;
    status(&pool, predecessor, Some(StatusOption::Completed)).await;
    let previous_value =
        serde_json::json!({"type":"SelectOption","value":[StatusOption::NOT_STARTED_UUID]});
    raw(
        &pool,
        source,
        SystemPropertyKey::STATUS_UUID,
        Some(previous_value.clone()),
    )
    .await;
    for target in [
        StatusOption::InProgress,
        StatusOption::InReview,
        StatusOption::Completed,
    ] {
        let outcome = transition_task_status(&pool, source, Some(target)).await?;
        assert!(
            matches!(outcome, TaskStatusMutationOutcome::Updated(ref snapshot)
            if snapshot.previous_value == Some(models_properties::service::property_value::PropertyValue::SelectOption(vec![StatusOption::NOT_STARTED_UUID]))
                && snapshot.value == Some(models_properties::service::property_value::PropertyValue::SelectOption(vec![target.uuid()])))
        );
        assert_eq!(
            status_row(&pool, source).await,
            (
                1,
                Some(serde_json::json!({"type":"SelectOption","value":[target.uuid()]}))
            )
        );
        raw(
            &pool,
            source,
            SystemPropertyKey::STATUS_UUID,
            Some(previous_value.clone()),
        )
        .await;
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn ungated_statuses_bypass_readiness_but_not_live_source(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let predecessor = Uuid::new_v4();
    task(&pool, source, None, true).await;
    task(&pool, predecessor, None, true).await;
    depends(&pool,source,serde_json::json!({"type":"EntityReference","value":[EntityReference::new(predecessor.to_string(),EntityType::Task)]})).await;
    for target in [
        Some(StatusOption::NotStarted),
        Some(StatusOption::Canceled),
        None,
    ] {
        assert!(matches!(
            transition_task_status(&pool, source, target).await?,
            TaskStatusMutationOutcome::Updated(_)
        ));
    }
    let non_task = Uuid::new_v4();
    task(&pool, non_task, None, false).await;
    assert!(matches!(
        transition_task_status(&pool, non_task, None).await?,
        TaskStatusMutationOutcome::Blocked
    ));
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn guarded_status_blocks_every_unavailable_or_malformed_predecessor(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let live = Uuid::new_v4();
    let other = Uuid::new_v4();
    let non_task = Uuid::new_v4();
    let deleted = Uuid::new_v4();
    task(&pool, source, Some("task-dependencies-project-a"), true).await;
    task(&pool, live, Some("task-dependencies-project-a"), true).await;
    task(&pool, other, Some("task-dependencies-project-b"), true).await;
    task(&pool, non_task, Some("task-dependencies-project-a"), false).await;
    task(&pool, deleted, Some("task-dependencies-project-a"), true).await;
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\"=NOW() WHERE id=$1")
        .bind(deleted.to_string())
        .execute(&pool)
        .await?;
    status(&pool, source, Some(StatusOption::NotStarted)).await;
    status(&pool, live, Some(StatusOption::Completed)).await;
    let known_source_status =
        serde_json::json!({"type":"SelectOption","value":[StatusOption::NOT_STARTED_UUID]});
    let cases = vec![
        serde_json::json!({"type":"EntityReference","value":[EntityReference::new(Uuid::new_v4().to_string(),EntityType::Task)]}),
        serde_json::json!({"type":"EntityReference","value":[EntityReference::new(deleted.to_string(),EntityType::Task)]}),
        serde_json::json!({"type":"EntityReference","value":[EntityReference::new(non_task.to_string(),EntityType::Task)]}),
        serde_json::json!({"type":"EntityReference","value":[EntityReference::new(other.to_string(),EntityType::Task)]}),
        serde_json::json!({"malformed":true}),
        serde_json::json!({"type":"String","value":"bad"}),
        serde_json::json!({"type":"EntityReference","value":[EntityReference::new(live.to_string(),EntityType::Document)]}),
        serde_json::json!({"type":"EntityReference","value":[EntityReference::new("not-a-uuid",EntityType::Task)]}),
        serde_json::json!({"type":"EntityReference","value":[EntityReference::with_message_id(live.to_string(),EntityType::Task,Uuid::new_v4())]}),
        serde_json::json!({"type":"EntityReference","value":[EntityReference::new(source.to_string(),EntityType::Task)]}),
    ];
    for value in cases {
        raw(
            &pool,
            source,
            SystemPropertyKey::STATUS_UUID,
            Some(known_source_status.clone()),
        )
        .await;
        depends(&pool, source, value).await;
        assert_eq!(
            blocked_readiness(
                transition_task_status(&pool, source, Some(StatusOption::InProgress)).await?,
            ),
            TaskDependencyReadiness {
                task_id: source,
                readiness: TaskReadiness::Blocked,
                depends_on_task_ids: vec![],
                blocking_task_ids: vec![],
                has_unavailable_dependencies: true,
            }
        );
        assert_eq!(
            status_row(&pool, source).await,
            (1, Some(known_source_status.clone()))
        );
    }
    raw(&pool, live, SystemPropertyKey::STATUS_UUID, None).await;
    raw(
        &pool,
        source,
        SystemPropertyKey::STATUS_UUID,
        Some(known_source_status.clone()),
    )
    .await;
    depends(&pool, source, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(live.to_string(),EntityType::Task)]})).await;
    assert_eq!(
        blocked_readiness(
            transition_task_status(&pool, source, Some(StatusOption::InProgress)).await?,
        ),
        TaskDependencyReadiness {
            task_id: source,
            readiness: TaskReadiness::Blocked,
            depends_on_task_ids: vec![live],
            blocking_task_ids: vec![live],
            has_unavailable_dependencies: false,
        }
    );
    assert_eq!(
        status_row(&pool, source).await,
        (1, Some(known_source_status.clone()))
    );
    raw(
        &pool,
        live,
        SystemPropertyKey::STATUS_UUID,
        Some(serde_json::json!({
            "type":"SelectOption",
            "value":[StatusOption::COMPLETED_UUID, StatusOption::IN_PROGRESS_UUID],
        })),
    )
    .await;
    raw(
        &pool,
        source,
        SystemPropertyKey::STATUS_UUID,
        Some(known_source_status.clone()),
    )
    .await;
    depends(&pool, source, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(live.to_string(),EntityType::Task)]})).await;
    assert_eq!(
        blocked_readiness(
            transition_task_status(&pool, source, Some(StatusOption::InProgress)).await?,
        ),
        TaskDependencyReadiness {
            task_id: source,
            readiness: TaskReadiness::Blocked,
            depends_on_task_ids: vec![live],
            blocking_task_ids: vec![live],
            has_unavailable_dependencies: false,
        }
    );
    assert_eq!(
        status_row(&pool, source).await,
        (1, Some(known_source_status.clone()))
    );
    for predecessor_status in [
        None,
        Some(StatusOption::NotStarted),
        Some(StatusOption::InProgress),
        Some(StatusOption::InReview),
        Some(StatusOption::Canceled),
    ] {
        raw(
            &pool,
            source,
            SystemPropertyKey::STATUS_UUID,
            Some(known_source_status.clone()),
        )
        .await;
        if predecessor_status.is_none() {
            sqlx::query("DELETE FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK' AND property_definition_id = $2")
                .bind(live.to_string())
                .bind(SystemPropertyKey::STATUS_UUID)
                .execute(&pool)
                .await?;
        } else {
            status(&pool, live, predecessor_status).await;
        }
        depends(&pool, source, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(live.to_string(),EntityType::Task)]})).await;
        assert_eq!(
            blocked_readiness(
                transition_task_status(&pool, source, Some(StatusOption::InProgress)).await?,
            ),
            TaskDependencyReadiness {
                task_id: source,
                readiness: TaskReadiness::Blocked,
                depends_on_task_ids: vec![live],
                blocking_task_ids: vec![live],
                has_unavailable_dependencies: false,
            }
        );
        assert_eq!(
            status_row(&pool, source).await,
            (1, Some(known_source_status.clone()))
        );
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn guarded_status_reports_mixed_live_dependencies_in_stored_order(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let completed = Uuid::new_v4();
    let incomplete = Uuid::new_v4();
    let unavailable = Uuid::new_v4();
    for id in [source, completed, incomplete] {
        task(&pool, id, None, true).await;
    }
    status(&pool, completed, Some(StatusOption::Completed)).await;
    status(&pool, incomplete, Some(StatusOption::InProgress)).await;
    depends(
        &pool,
        source,
        serde_json::json!({"type":"EntityReference","value":[
            EntityReference::new(completed.to_string(), EntityType::Task),
            EntityReference::new(incomplete.to_string(), EntityType::Task),
            EntityReference::new(unavailable.to_string(), EntityType::Task)
        ]}),
    )
    .await;
    assert_eq!(
        blocked_readiness(
            transition_task_status(&pool, source, Some(StatusOption::InProgress)).await?,
        ),
        TaskDependencyReadiness {
            task_id: source,
            readiness: TaskReadiness::Blocked,
            depends_on_task_ids: vec![completed, incomplete],
            blocking_task_ids: vec![incomplete],
            has_unavailable_dependencies: true,
        }
    );
    Ok(())
}
