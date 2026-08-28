use std::sync::Arc;

use models_properties::service::property_value::PropertyValue;
use sqlx::{Pool, Postgres};
use tokio::sync::Barrier;
use uuid::Uuid;

use super::replace_task_dependencies;
use crate::domain::model::TaskDependencyMutationOutcome;
use crate::outbound::task_status_transition_queries::transition_task_status;
use macro_db_migrator::MACRO_DB_MIGRATIONS;

async fn task(pool: &Pool<Postgres>, id: Uuid, project: Option<&str>, is_task: bool) {
    sqlx::query("INSERT INTO \"Document\" (id, name, owner, \"projectId\") VALUES ($1, 'task', 'task-dependencies-owner', $2)")
        .bind(id.to_string())
        .bind(project)
        .execute(pool)
        .await
        .unwrap();
    if is_task {
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
            .bind(id.to_string())
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn depends(pool: &Pool<Postgres>, id: Uuid) -> Option<PropertyValue> {
    sqlx::query_scalar::<_, Option<serde_json::Value>>(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK' AND property_definition_id = $2",
    )
    .bind(id.to_string())
    .bind(system_properties::SystemPropertyKey::DEPENDS_ON_UUID)
    .fetch_optional(pool)
    .await
    .unwrap()
    .flatten()
    .map(serde_json::from_value)
    .transpose()
    .unwrap()
}

async fn status(pool: &Pool<Postgres>, id: Uuid) -> Option<PropertyValue> {
    sqlx::query_scalar::<_, Option<serde_json::Value>>(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK' AND property_definition_id = $2",
    )
    .bind(id.to_string())
    .bind(system_properties::SystemPropertyKey::STATUS_UUID)
    .fetch_optional(pool)
    .await
    .unwrap()
    .flatten()
    .map(serde_json::from_value)
    .transpose()
    .unwrap()
}

async fn store_raw_depends(pool: &Pool<Postgres>, id: Uuid, value: serde_json::Value) {
    sqlx::query(
        r#"
        INSERT INTO entity_properties
            (id, entity_id, entity_type, property_definition_id, values)
        VALUES ($1, $2, 'TASK', $3, $4)
        ON CONFLICT (entity_id, entity_type, property_definition_id)
        DO UPDATE SET values = EXCLUDED.values
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(id.to_string())
    .bind(system_properties::SystemPropertyKey::DEPENDS_ON_UUID)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

async fn store_status(pool: &Pool<Postgres>, id: Uuid, status: system_properties::StatusOption) {
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4) ON CONFLICT (entity_id, entity_type, property_definition_id) DO UPDATE SET values = EXCLUDED.values")
        .bind(Uuid::new_v4()).bind(id.to_string()).bind(system_properties::SystemPropertyKey::STATUS_UUID)
        .bind(serde_json::json!({"type":"SelectOption","value":[status.uuid()]})).execute(pool).await.unwrap();
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn replacement_is_canonical_ordered_and_clears(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    for id in [source, first, second] {
        task(&pool, id, Some("task-dependencies-project-a"), true).await;
    }
    assert!(matches!(
        replace_task_dependencies(&pool, source, &[second, first]).await?,
        TaskDependencyMutationOutcome::Updated(_)
    ));
    assert!(
        matches!(depends(&pool, source).await, Some(PropertyValue::EntityRef(refs)) if refs.iter().map(|r| r.entity_id.as_str()).collect::<Vec<_>>() == vec![second.to_string(), first.to_string()])
    );
    let replacement = replace_task_dependencies(&pool, source, &[first]).await?;
    assert!(matches!(
        replacement,
        TaskDependencyMutationOutcome::Updated(ref snapshot)
            if matches!(snapshot.previous_value, Some(PropertyValue::EntityRef(ref refs)) if refs.len() == 2)
    ));
    assert!(
        matches!(depends(&pool, source).await, Some(PropertyValue::EntityRef(refs)) if refs.iter().map(|r| r.entity_id.as_str()).collect::<Vec<_>>() == vec![first.to_string()])
    );
    assert!(
        matches!(replace_task_dependencies(&pool, source, &[]).await?, TaskDependencyMutationOutcome::Updated(snapshot) if snapshot.value.is_none())
    );
    assert_eq!(depends(&pool, source).await, None);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn legacy_cycles_and_malformed_non_task_edges_terminate_without_false_cycles(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let legacy_a = Uuid::new_v4();
    let legacy_b = Uuid::new_v4();
    let malformed = Uuid::new_v4();
    let non_task_edge = Uuid::new_v4();
    for id in [source, legacy_a, legacy_b, malformed, non_task_edge] {
        task(&pool, id, Some("task-dependencies-project-a"), true).await;
    }
    store_raw_depends(
        &pool,
        legacy_a,
        serde_json::json!({
            "type": "EntityReference",
            "value": [{"entity_id": legacy_b, "entity_type": "TASK"}],
        }),
    )
    .await;
    store_raw_depends(
        &pool,
        legacy_b,
        serde_json::json!({
            "type": "EntityReference",
            "value": [{"entity_id": legacy_a, "entity_type": "TASK"}],
        }),
    )
    .await;
    store_raw_depends(
        &pool,
        malformed,
        serde_json::json!({"type": "EntityReference", "value": [{"legacy": true}]}),
    )
    .await;
    store_raw_depends(
        &pool,
        non_task_edge,
        serde_json::json!({
            "type": "EntityReference",
            "value": [{"entity_id": source, "entity_type": "DOCUMENT"}],
        }),
    )
    .await;

    for dependency in [legacy_a, malformed, non_task_edge] {
        assert!(matches!(
            replace_task_dependencies(&pool, source, &[dependency]).await?,
            TaskDependencyMutationOutcome::Updated(_)
        ));
    }
    assert!(
        matches!(depends(&pool, source).await, Some(PropertyValue::EntityRef(refs)) if refs[0].entity_id == non_task_edge.to_string())
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn unavailable_targets_and_cycles_do_not_replace_prior_value(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let dependency = Uuid::new_v4();
    let third = Uuid::new_v4();
    let other_project = Uuid::new_v4();
    let document = Uuid::new_v4();
    let deleted = Uuid::new_v4();
    for id in [source, dependency, third] {
        task(&pool, id, Some("task-dependencies-project-a"), true).await;
    }
    task(
        &pool,
        other_project,
        Some("task-dependencies-project-b"),
        true,
    )
    .await;
    task(&pool, deleted, Some("task-dependencies-project-a"), true).await;
    sqlx::query("UPDATE \"Document\" SET \"deletedAt\" = NOW() WHERE id = $1")
        .bind(deleted.to_string())
        .execute(&pool)
        .await?;
    task(&pool, document, Some("task-dependencies-project-a"), false).await;
    replace_task_dependencies(&pool, source, &[dependency]).await?;
    for bad in [Uuid::new_v4(), deleted, other_project, document] {
        assert!(matches!(
            replace_task_dependencies(&pool, source, &[bad]).await?,
            TaskDependencyMutationOutcome::Unavailable
        ));
        assert!(
            matches!(depends(&pool, source).await, Some(PropertyValue::EntityRef(refs)) if refs[0].entity_id == dependency.to_string())
        );
    }
    assert!(matches!(
        replace_task_dependencies(&pool, dependency, &[source]).await?,
        TaskDependencyMutationOutcome::Cycle
    ));
    assert!(matches!(depends(&pool, dependency).await, None));
    replace_task_dependencies(&pool, dependency, &[third]).await?;
    assert!(matches!(
        replace_task_dependencies(&pool, third, &[source]).await?,
        TaskDependencyMutationOutcome::Cycle
    ));
    assert!(depends(&pool, third).await.is_none());
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn null_projects_match_and_concurrent_reverse_edges_leave_a_dag(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    task(&pool, a, None, true).await;
    task(&pool, b, None, true).await;
    let barrier = Arc::new(Barrier::new(2));
    let one = {
        let barrier = barrier.clone();
        let pool = pool.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            replace_task_dependencies(&pool, a, &[b]).await
        })
    };
    let two = {
        let barrier = barrier.clone();
        let pool = pool.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            replace_task_dependencies(&pool, b, &[a]).await
        })
    };
    let outcomes = [one.await??, two.await??];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, TaskDependencyMutationOutcome::Updated(_)))
            .count(),
        1
    );
    let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity_properties WHERE entity_type = 'TASK' AND property_definition_id = $1 AND values IS NOT NULL")
        .bind(system_properties::SystemPropertyKey::DEPENDS_ON_UUID).fetch_one(&pool).await?;
    assert_eq!(edge_count, 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, TaskDependencyMutationOutcome::Cycle))
            .count(),
        1
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn guarded_source_rejects_incomplete_replacement_and_retains_prior_value(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let complete = Uuid::new_v4();
    let incomplete = Uuid::new_v4();
    let canceled = Uuid::new_v4();
    for id in [source, complete, incomplete, canceled] {
        task(&pool, id, None, true).await;
    }
    store_status(&pool, complete, system_properties::StatusOption::Completed).await;
    store_status(&pool, canceled, system_properties::StatusOption::Canceled).await;
    for source_status in [
        system_properties::StatusOption::InProgress,
        system_properties::StatusOption::InReview,
        system_properties::StatusOption::Completed,
    ] {
        store_status(&pool, source, source_status).await;
        replace_task_dependencies(&pool, source, &[complete]).await?;
        for candidate in [incomplete, canceled] {
            assert!(matches!(
                replace_task_dependencies(&pool, source, &[candidate]).await?,
                TaskDependencyMutationOutcome::Blocked
            ));
            assert_eq!(
                depends(&pool, source).await,
                Some(PropertyValue::EntityRef(vec![
                    models_properties::EntityReference::new(
                        complete.to_string(),
                        models_properties::EntityType::Task,
                    )
                ]))
            );
        }
        assert!(matches!(
            replace_task_dependencies(&pool, source, &[complete]).await?,
            TaskDependencyMutationOutcome::Updated(_)
        ));
        assert!(matches!(
            replace_task_dependencies(&pool, source, &[]).await?,
            TaskDependencyMutationOutcome::Updated(snapshot) if snapshot.value.is_none()
        ));
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn unguarded_source_allows_incomplete_replacement(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let incomplete = Uuid::new_v4();
    task(&pool, incomplete, None, true).await;
    for source_status in [
        system_properties::StatusOption::NotStarted,
        system_properties::StatusOption::Canceled,
    ] {
        let source = Uuid::new_v4();
        task(&pool, source, None, true).await;
        store_status(&pool, source, source_status).await;
        assert!(matches!(
            replace_task_dependencies(&pool, source, &[incomplete]).await?,
            TaskDependencyMutationOutcome::Updated(_)
        ));
        assert_eq!(
            depends(&pool, source).await,
            Some(PropertyValue::EntityRef(vec![
                models_properties::EntityReference::new(
                    incomplete.to_string(),
                    models_properties::EntityType::Task,
                )
            ]))
        );
    }
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("task_dependencies_seed"))
)]
async fn status_and_dependency_race_commits_only_one_side(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let source = Uuid::new_v4();
    let incomplete = Uuid::new_v4();
    for id in [source, incomplete] {
        task(&pool, id, None, true).await;
    }
    store_status(&pool, source, system_properties::StatusOption::NotStarted).await;
    let barrier = Arc::new(Barrier::new(2));
    let status_task = {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            transition_task_status(
                &pool,
                source,
                Some(system_properties::StatusOption::InProgress),
            )
            .await
        })
    };
    let deps = {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            replace_task_dependencies(&pool, source, &[incomplete]).await
        })
    };
    let status_outcome = status_task.await??;
    let deps = deps.await??;
    assert_eq!(
        usize::from(matches!(
            status_outcome,
            crate::domain::model::TaskStatusMutationOutcome::Updated(_)
        )) + usize::from(matches!(deps, TaskDependencyMutationOutcome::Updated(_))),
        1
    );
    assert_eq!(
        usize::from(matches!(
            status_outcome,
            crate::domain::model::TaskStatusMutationOutcome::Blocked
        )) + usize::from(matches!(deps, TaskDependencyMutationOutcome::Blocked)),
        1
    );
    let final_status = status(&pool, source).await;
    let final_dependencies = depends(&pool, source).await;
    assert!(
        (final_status
            == Some(PropertyValue::SelectOption(vec![
                system_properties::StatusOption::IN_PROGRESS_UUID,
            ]))
            && final_dependencies.is_none())
            || (final_status
                == Some(PropertyValue::SelectOption(vec![
                    system_properties::StatusOption::NOT_STARTED_UUID,
                ]))
                && final_dependencies
                    == Some(PropertyValue::EntityRef(vec![
                        models_properties::EntityReference::new(
                            incomplete.to_string(),
                            models_properties::EntityType::Task,
                        ),
                    ])))
    );
    Ok(())
}
