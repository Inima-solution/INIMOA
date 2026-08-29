use super::transition_task_status;
use crate::domain::model::{TaskDependencyReadiness, TaskReadiness, TaskStatusMutationOutcome};
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use models_properties::{EntityReference, EntityType};
use sqlx::{Pool, Postgres};
use system_properties::{StatusOption, SystemPropertyKey};
use tokio::time::{Duration, timeout};
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

fn blocked_subtask_readiness(
    outcome: TaskStatusMutationOutcome,
) -> crate::domain::model::TaskSubtaskCompletionReadiness {
    let TaskStatusMutationOutcome::BlockedBySubtasks(readiness) = outcome else {
        panic!("expected subtask completion blocker");
    };
    readiness
}

async fn subtasks(pool: &Pool<Postgres>, parent: Uuid, children: &[Uuid]) {
    raw(
        pool,
        parent,
        SystemPropertyKey::SUBTASKS_UUID,
        Some(serde_json::json!({"type":"EntityReference","value": children.iter().map(|id| EntityReference::new(id.to_string(), EntityType::Task)).collect::<Vec<_>>() })),
    )
    .await;
}

async fn parent(pool: &Pool<Postgres>, child: Uuid, parent: Uuid) {
    raw(
        pool,
        child,
        SystemPropertyKey::PARENT_TASK_UUID,
        Some(serde_json::json!({"type":"EntityReference","value":[EntityReference::new(parent.to_string(), EntityType::Task)]})),
    )
    .await;
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn completion_requires_canonical_terminal_subtasks_in_source_order(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let completed = Uuid::new_v4();
    let canceled = Uuid::new_v4();
    let blocked = Uuid::new_v4();
    for id in [source, completed, canceled, blocked] {
        task(&pool, id, Some("task-dependencies-project-a"), true).await;
    }
    subtasks(&pool, source, &[canceled, completed, blocked]).await;
    for child in [completed, canceled, blocked] {
        parent(&pool, child, source).await;
    }
    status(&pool, completed, Some(StatusOption::Completed)).await;
    status(&pool, canceled, Some(StatusOption::Canceled)).await;
    let readiness = blocked_subtask_readiness(
        transition_task_status(&pool, source, Some(StatusOption::Completed)).await?,
    );
    assert_eq!(readiness.subtask_ids, vec![canceled, completed, blocked]);
    assert_eq!(readiness.blocking_subtask_ids, vec![blocked]);
    assert!(!readiness.has_unavailable_subtasks);
    assert_eq!(status_row(&pool, source).await, (0, None));
    status(&pool, blocked, Some(StatusOption::Completed)).await;
    assert!(matches!(
        transition_task_status(&pool, source, Some(StatusOption::Completed)).await?,
        TaskStatusMutationOutcome::Updated(_)
    ));
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn completion_without_subtasks_row_succeeds(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let parent = Uuid::new_v4();
    task(&pool, parent, Some("task-dependencies-project-a"), true).await;
    assert!(matches!(
        transition_task_status(&pool, parent, Some(StatusOption::Completed)).await?,
        TaskStatusMutationOutcome::Updated(_)
    ));
    assert_eq!(
        status_row(&pool, parent).await,
        (
            1,
            Some(serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]}))
        )
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn child_completion_never_changes_canonical_parent_status(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    for id in [parent_id, child_id] {
        task(&pool, id, Some("task-dependencies-project-a"), true).await;
    }
    subtasks(&pool, parent_id, &[child_id]).await;
    parent(&pool, child_id, parent_id).await;
    let parent_prior =
        serde_json::json!({"type":"SelectOption","value":[StatusOption::IN_PROGRESS_UUID]});
    raw(
        &pool,
        parent_id,
        SystemPropertyKey::STATUS_UUID,
        Some(parent_prior.clone()),
    )
    .await;
    assert!(matches!(
        transition_task_status(&pool, child_id, Some(StatusOption::Completed)).await?,
        TaskStatusMutationOutcome::Updated(_)
    ));
    assert_eq!(
        status_row(&pool, child_id).await,
        (
            1,
            Some(serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]}))
        )
    );
    assert_eq!(status_row(&pool, parent_id).await, (1, Some(parent_prior)));
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn unrelated_parent_message_id_does_not_poison_no_child_completion(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let parent_id = Uuid::new_v4();
    let unrelated = Uuid::new_v4();
    task(&pool, parent_id, Some("task-dependencies-project-a"), true).await;
    task(&pool, unrelated, Some("task-dependencies-project-a"), true).await;
    raw(&pool, unrelated, SystemPropertyKey::PARENT_TASK_UUID, Some(serde_json::json!({
        "type":"EntityReference",
        "value":[EntityReference::with_message_id(Uuid::new_v4().to_string(), EntityType::Task, parent_id)]
    }))).await;
    assert!(matches!(
        transition_task_status(&pool, parent_id, Some(StatusOption::Completed)).await?,
        TaskStatusMutationOutcome::Updated(_)
    ));
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn taskdeps_lock_waits_for_committed_child_status_before_parent_completion(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    for id in [parent_id, child_id] {
        task(&pool, id, Some("task-dependencies-project-a"), true).await;
    }
    subtasks(&pool, parent_id, &[child_id]).await;
    parent(&pool, child_id, parent_id).await;
    status(&pool, child_id, Some(StatusOption::InProgress)).await;
    let mut tx = pool.begin().await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE entity_properties SET values = $1 WHERE entity_id = $2 AND entity_type = 'TASK' AND property_definition_id = $3")
        .bind(serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID]}))
        .bind(child_id.to_string()).bind(SystemPropertyKey::STATUS_UUID).execute(&mut *tx).await?;
    let completion_pool = pool.clone();
    let completion = tokio::spawn(async move {
        transition_task_status(&completion_pool, parent_id, Some(StatusOption::Completed)).await
    });
    tokio::task::yield_now().await;
    tx.commit().await?;
    let completion = timeout(Duration::from_secs(5), completion).await??;
    assert!(matches!(completion?, TaskStatusMutationOutcome::Updated(_)));
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn taskhier_lock_and_hierarchy_move_complete_without_deadlock_and_use_committed_snapshot(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    for id in [parent_id, child_id] {
        task(&pool, id, Some("task-dependencies-project-a"), true).await;
    }
    status(&pool, child_id, Some(StatusOption::InProgress)).await;
    let mut tx = pool.begin().await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKHIER")
    )
    .fetch_one(&mut *tx)
    .await?;
    let completion_pool = pool.clone();
    let completion = tokio::spawn(async move {
        transition_task_status(&completion_pool, parent_id, Some(StatusOption::Completed)).await
    });
    tokio::task::yield_now().await;
    timeout(Duration::from_secs(5), async {
        sqlx::query("SELECT id FROM \"Document\" WHERE id = $1 FOR UPDATE")
            .bind(parent_id.to_string())
            .fetch_one(&mut *tx)
            .await?;
        let subtask_value = serde_json::json!({"type":"EntityReference","value":[EntityReference::new(child_id.to_string(), EntityType::Task)]});
        let parent_value = serde_json::json!({"type":"EntityReference","value":[EntityReference::new(parent_id.to_string(), EntityType::Task)]});
        for (entity_id, definition, value) in [
            (parent_id, SystemPropertyKey::SUBTASKS_UUID, subtask_value),
            (child_id, SystemPropertyKey::PARENT_TASK_UUID, parent_value),
        ] {
            sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4) ON CONFLICT (entity_id, entity_type, property_definition_id) DO UPDATE SET values = EXCLUDED.values")
                .bind(Uuid::new_v4()).bind(entity_id.to_string()).bind(definition).bind(value).execute(&mut *tx).await?;
        }
        Ok::<(), anyhow::Error>(())
    }).await??;
    tx.commit().await?;
    let completion = timeout(Duration::from_secs(5), completion).await??;
    assert_eq!(
        blocked_subtask_readiness(completion?),
        crate::domain::model::TaskSubtaskCompletionReadiness {
            task_id: parent_id,
            readiness: TaskReadiness::Blocked,
            subtask_ids: vec![child_id],
            blocking_subtask_ids: vec![child_id],
            has_unavailable_subtasks: false,
        }
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn completion_subtask_status_matrix_blocks_exact_live_reciprocal_child(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let child = Uuid::new_v4();
    task(&pool, source, Some("task-dependencies-project-a"), true).await;
    task(&pool, child, Some("task-dependencies-project-a"), true).await;
    subtasks(&pool, source, &[child]).await;
    parent(&pool, child, source).await;
    let prior = serde_json::json!({"type":"SelectOption","value":[StatusOption::NOT_STARTED_UUID]});
    sqlx::query("ALTER TABLE entity_properties DROP CONSTRAINT check_values_structure")
        .execute(&pool)
        .await?;
    let cases = [
        Some(serde_json::json!({"type":"SelectOption","value":[StatusOption::NOT_STARTED_UUID]})),
        Some(serde_json::json!({"type":"SelectOption","value":[StatusOption::IN_PROGRESS_UUID]})),
        Some(serde_json::json!({"type":"SelectOption","value":[StatusOption::IN_REVIEW_UUID]})),
        None,
        Some(serde_json::Value::Null),
        Some(serde_json::json!({"malformed":true})),
        Some(
            serde_json::json!({"type":"SelectOption","value":[StatusOption::COMPLETED_UUID, StatusOption::IN_PROGRESS_UUID]}),
        ),
        Some(serde_json::json!({"type":"SelectOption","value":[Uuid::new_v4()]})),
    ];
    for child_status in cases {
        raw(
            &pool,
            source,
            SystemPropertyKey::STATUS_UUID,
            Some(prior.clone()),
        )
        .await;
        raw(&pool, child, SystemPropertyKey::STATUS_UUID, child_status).await;
        assert_eq!(
            blocked_subtask_readiness(
                transition_task_status(&pool, source, Some(StatusOption::Completed)).await?
            ),
            crate::domain::model::TaskSubtaskCompletionReadiness {
                task_id: source,
                readiness: TaskReadiness::Blocked,
                subtask_ids: vec![child],
                blocking_subtask_ids: vec![child],
                has_unavailable_subtasks: false,
            }
        );
        assert_eq!(status_row(&pool, source).await, (1, Some(prior.clone())));
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn completion_subtask_relationship_failures_are_unavailable_and_omit_ids(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let child = Uuid::new_v4();
    let other_project = Uuid::new_v4();
    let non_task = Uuid::new_v4();
    let deleted = Uuid::new_v4();
    for id in [source, child, non_task, deleted] {
        task(
            &pool,
            id,
            Some("task-dependencies-project-a"),
            id != non_task,
        )
        .await;
    }
    task(
        &pool,
        other_project,
        Some("task-dependencies-project-b"),
        true,
    )
    .await;
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\" = NOW() WHERE id = $1")
        .bind(deleted.to_string())
        .execute(&pool)
        .await?;
    sqlx::query("ALTER TABLE entity_properties DROP CONSTRAINT check_values_structure")
        .execute(&pool)
        .await?;
    let unavailable = |outcome| {
        assert_eq!(
            blocked_subtask_readiness(outcome),
            crate::domain::model::TaskSubtaskCompletionReadiness {
                task_id: source,
                readiness: TaskReadiness::Blocked,
                subtask_ids: vec![],
                blocking_subtask_ids: vec![],
                has_unavailable_subtasks: true,
            }
        );
    };
    for candidate in [Uuid::new_v4(), deleted, non_task, other_project] {
        subtasks(&pool, source, &[candidate]).await;
        unavailable(transition_task_status(&pool, source, Some(StatusOption::Completed)).await?);
    }
    // Source is authoritative: malformed/duplicate children fail closed before any IDs are exposed.
    raw(
        &pool,
        source,
        SystemPropertyKey::SUBTASKS_UUID,
        Some(serde_json::json!({"malformed":true})),
    )
    .await;
    unavailable(transition_task_status(&pool, source, Some(StatusOption::Completed)).await?);
    raw(&pool, source, SystemPropertyKey::SUBTASKS_UUID, Some(serde_json::json!({"type":"EntityReference","value":[EntityReference::new(child.to_string(), EntityType::Task), EntityReference::new(child.to_string(), EntityType::Task)]}))).await;
    unavailable(transition_task_status(&pool, source, Some(StatusOption::Completed)).await?);
    // A forward-only edge, a reverse-only edge, and a non-exact reciprocal are all unavailable.
    subtasks(&pool, source, &[child]).await;
    unavailable(transition_task_status(&pool, source, Some(StatusOption::Completed)).await?);
    raw(&pool, source, SystemPropertyKey::SUBTASKS_UUID, None).await;
    parent(&pool, child, source).await;
    unavailable(transition_task_status(&pool, source, Some(StatusOption::Completed)).await?);
    subtasks(&pool, source, &[child]).await;
    raw(&pool, child, SystemPropertyKey::PARENT_TASK_UUID, Some(serde_json::json!({"type":"EntityReference","value":[EntityReference::new(source.to_string(), EntityType::Task), EntityReference::new(Uuid::new_v4().to_string(), EntityType::Task)]}))).await;
    unavailable(transition_task_status(&pool, source, Some(StatusOption::Completed)).await?);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn completed_retry_revalidates_subtasks_and_dependency_precedes_subtasks(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let child = Uuid::new_v4();
    let dependency = Uuid::new_v4();
    for id in [source, child, dependency] {
        task(&pool, id, Some("task-dependencies-project-a"), true).await;
    }
    subtasks(&pool, source, &[child]).await;
    parent(&pool, child, source).await;
    status(&pool, child, Some(StatusOption::Completed)).await;
    assert!(matches!(
        transition_task_status(&pool, source, Some(StatusOption::Completed)).await?,
        TaskStatusMutationOutcome::Updated(_)
    ));
    status(&pool, child, Some(StatusOption::InProgress)).await;
    assert_eq!(
        blocked_subtask_readiness(
            transition_task_status(&pool, source, Some(StatusOption::Completed)).await?
        ),
        crate::domain::model::TaskSubtaskCompletionReadiness {
            task_id: source,
            readiness: TaskReadiness::Blocked,
            subtask_ids: vec![child],
            blocking_subtask_ids: vec![child],
            has_unavailable_subtasks: false,
        }
    );
    depends(&pool, source, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(dependency.to_string(), EntityType::Task)]})).await;
    assert_eq!(
        blocked_readiness(
            transition_task_status(&pool, source, Some(StatusOption::Completed)).await?
        ),
        TaskDependencyReadiness {
            task_id: source,
            readiness: TaskReadiness::Blocked,
            depends_on_task_ids: vec![dependency],
            blocking_task_ids: vec![dependency],
            has_unavailable_dependencies: false,
        }
    );
    Ok(())
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

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn shared_final_ready_fanout_is_distinct_and_uuid_ordered(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let predecessor = Uuid::new_v4();
    let ready = Uuid::new_v4();
    let explicit_null = Uuid::new_v4();
    let blocked = Uuid::new_v4();
    let incomplete = Uuid::new_v4();
    let deleted_case = Uuid::new_v4();
    let deleted_dependency = Uuid::new_v4();
    let missing_case = Uuid::new_v4();
    let non_task_case = Uuid::new_v4();
    let non_task_dependency = Uuid::new_v4();
    let cross_project_case = Uuid::new_v4();
    let cross_project_dependency = Uuid::new_v4();
    let malformed_top_level = Uuid::new_v4();
    let self_reference = Uuid::new_v4();
    let message_reference = Uuid::new_v4();
    for id in [
        predecessor,
        ready,
        explicit_null,
        blocked,
        incomplete,
        deleted_case,
        deleted_dependency,
        missing_case,
        non_task_case,
        cross_project_case,
        malformed_top_level,
        self_reference,
        message_reference,
    ] {
        task(&pool, id, Some("task-dependencies-project-a"), true).await;
    }
    task(
        &pool,
        non_task_dependency,
        Some("task-dependencies-project-a"),
        false,
    )
    .await;
    task(
        &pool,
        cross_project_dependency,
        Some("task-dependencies-project-b"),
        true,
    )
    .await;
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\" = NOW() WHERE id = $1")
        .bind(deleted_dependency.to_string())
        .execute(&pool)
        .await?;
    status(&pool, predecessor, Some(StatusOption::NotStarted)).await;
    status(&pool, incomplete, Some(StatusOption::InProgress)).await;
    depends(&pool, ready, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(predecessor.to_string(), EntityType::Task), EntityReference::new(predecessor.to_string(), EntityType::Task)]})).await;
    depends(&pool, explicit_null, serde_json::json!({"type":"EntityReference","value":[{"entity_id": predecessor, "entity_type":"TASK", "specific_message_id": null}]})).await;
    depends(&pool, blocked, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(predecessor.to_string(), EntityType::Task), EntityReference::new(incomplete.to_string(), EntityType::Task)]})).await;
    depends(&pool, deleted_case, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(predecessor.to_string(), EntityType::Task), EntityReference::new(deleted_dependency.to_string(), EntityType::Task)]})).await;
    depends(&pool, missing_case, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(predecessor.to_string(), EntityType::Task), EntityReference::new(Uuid::new_v4().to_string(), EntityType::Task)]})).await;
    depends(&pool, non_task_case, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(predecessor.to_string(), EntityType::Task), EntityReference::new(non_task_dependency.to_string(), EntityType::Task)]})).await;
    depends(&pool, cross_project_case, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(predecessor.to_string(), EntityType::Task), EntityReference::new(cross_project_dependency.to_string(), EntityType::Task)]})).await;
    sqlx::query("ALTER TABLE entity_properties DROP CONSTRAINT check_values_structure")
        .execute(&pool)
        .await?;
    depends(&pool, malformed_top_level, serde_json::json!({"type":"String","value":[EntityReference::new(predecessor.to_string(), EntityType::Task)]})).await;
    depends(&pool, self_reference, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(predecessor.to_string(), EntityType::Task), EntityReference::new(self_reference.to_string(), EntityType::Task)]})).await;
    depends(&pool, message_reference, serde_json::json!({"type":"EntityReference","value":[EntityReference::new(predecessor.to_string(), EntityType::Task), EntityReference::with_message_id(ready.to_string(), EntityType::Task, Uuid::new_v4())]})).await;

    let TaskStatusMutationOutcome::UpdatedWithReady { ready_task_ids, .. } =
        transition_task_status(&pool, predecessor, Some(StatusOption::Completed)).await?
    else {
        panic!("expected final-ready fanout");
    };
    let mut expected = vec![ready, explicit_null];
    expected.sort_unstable();
    assert_eq!(ready_task_ids, expected);
    assert!(
        ready_task_ids.contains(&ready),
        "completion passes no restored-source exclusion to the shared fanout"
    );
    // Retry and leaving Completed are ordinary writes, never readiness signals.
    assert!(matches!(
        transition_task_status(&pool, predecessor, Some(StatusOption::Completed)).await?,
        TaskStatusMutationOutcome::Updated(_)
    ));
    assert!(matches!(
        transition_task_status(&pool, predecessor, Some(StatusOption::NotStarted)).await?,
        TaskStatusMutationOutcome::Updated(_)
    ));
    Ok(())
}
