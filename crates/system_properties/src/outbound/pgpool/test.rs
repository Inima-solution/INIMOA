//! Tests for system properties PostgreSQL repository.

use models_properties::EntityType;

use super::*;
use crate::domain::model::{DecisionStateOption, PropertyRow, SystemPropertyKey};
use crate::domain::service::{SystemPropertiesService, SystemPropertiesServiceImpl};
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use sqlx::{Pool, Postgres};

/// Helper to count task properties
async fn count_task_properties(pool: &Pool<Postgres>, entity_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK'",
    )
    .bind(entity_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Helper to get task property values
async fn get_task_property_values(
    pool: &Pool<Postgres>,
    entity_id: &str,
) -> Vec<(Uuid, Option<serde_json::Value>)> {
    sqlx::query_as::<_, (Uuid, Option<serde_json::Value>)>(
        "SELECT property_definition_id, values FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK' ORDER BY property_definition_id",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

// ============================================================================
// bulk_upsert_properties tests
// ============================================================================

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn test_bulk_upsert_properties_insert(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());

    let entity_id = "test-task-insert";
    let rows = vec![
        PropertyRow::null_value(
            entity_id,
            EntityType::Task,
            SystemPropertyKey::Status.uuid(),
        ),
        PropertyRow::null_value(
            entity_id,
            EntityType::Task,
            SystemPropertyKey::Priority.uuid(),
        ),
    ];

    repo.bulk_upsert_properties(rows).await?;

    let count = count_task_properties(&pool, entity_id).await;
    assert_eq!(count, 2, "Should have inserted 2 properties");

    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("system_properties"))
)]
async fn test_bulk_upsert_properties_update(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());

    let entity_id = "source-task-with-props";
    let custom_notes_id: Uuid = "cccccccc-cccc-cccc-cccc-cccccccccc01".parse().unwrap();

    // Update Custom Notes (STRING type) - already has "This is a custom note" from fixture
    let rows = vec![PropertyRow::string_value(
        entity_id,
        EntityType::Task,
        custom_notes_id,
        "Updated note",
    )];

    repo.bulk_upsert_properties(rows).await?;

    let properties = get_task_property_values(&pool, entity_id).await;
    let notes_prop = properties.iter().find(|(id, _)| *id == custom_notes_id);

    assert_eq!(
        notes_prop.unwrap().1.as_ref().unwrap(),
        &serde_json::json!({"type": "String", "value": "Updated note"}),
        "Custom Notes should be updated"
    );

    Ok(())
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn test_bulk_upsert_properties_empty(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());

    // Empty input should succeed without error
    repo.bulk_upsert_properties(vec![]).await?;

    Ok(())
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn test_bulk_upsert_properties_multiple_entities(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());

    let rows = vec![
        PropertyRow::null_value("task-a", EntityType::Task, SystemPropertyKey::Status.uuid()),
        PropertyRow::null_value(
            "task-a",
            EntityType::Task,
            SystemPropertyKey::Priority.uuid(),
        ),
        PropertyRow::null_value("task-b", EntityType::Task, SystemPropertyKey::Status.uuid()),
        PropertyRow::null_value("task-c", EntityType::Task, SystemPropertyKey::Status.uuid()),
    ];

    repo.bulk_upsert_properties(rows).await?;

    assert_eq!(count_task_properties(&pool, "task-a").await, 2);
    assert_eq!(count_task_properties(&pool, "task-b").await, 1);
    assert_eq!(count_task_properties(&pool, "task-c").await, 1);

    Ok(())
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn test_bulk_insert_properties_if_absent_keeps_first_write(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());
    let entity_id = "email-doc-first-write";
    let subject_id = SystemPropertyKey::Subject.uuid();

    repo.bulk_insert_properties_if_absent(vec![PropertyRow::string_value(
        entity_id,
        EntityType::Document,
        subject_id,
        "original subject",
    )])
    .await?;
    repo.bulk_insert_properties_if_absent(vec![PropertyRow::string_value(
        entity_id,
        EntityType::Document,
        subject_id,
        "forwarded subject",
    )])
    .await?;

    let stored = sqlx::query_scalar!(
        r#"
        SELECT values
        FROM entity_properties
        WHERE entity_id = $1 AND property_definition_id = $2
        "#,
        entity_id,
        subject_id,
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(
        stored.unwrap(),
        serde_json::json!({"type": "String", "value": "original subject"})
    );

    Ok(())
}

// ============================================================================
// attach_task_properties tests
// ============================================================================

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn test_attach_task_properties_creates_exactly_all_required_null_rows(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let service = SystemPropertiesServiceImpl::new(PgSystemPropertiesRepository::new(pool.clone()));
    let task_id = "task-with-attached-system-properties";

    service
        .attach_task_properties(vec![task_id.to_owned()])
        .await?;

    assert_eq!(count_task_properties(&pool, task_id).await, 12);
    assert_eq!(
        get_task_property_values(&pool, task_id).await,
        vec![
            (
                SystemPropertyKey::Assignees.uuid(),
                Some(serde_json::Value::Null)
            ),
            (
                SystemPropertyKey::Status.uuid(),
                Some(serde_json::Value::Null)
            ),
            (
                SystemPropertyKey::Priority.uuid(),
                Some(serde_json::Value::Null)
            ),
            (
                SystemPropertyKey::DueDate.uuid(),
                Some(serde_json::Value::Null)
            ),
            (
                SystemPropertyKey::ParentTask.uuid(),
                Some(serde_json::Value::Null)
            ),
            (
                SystemPropertyKey::Subtasks.uuid(),
                Some(serde_json::Value::Null)
            ),
            (
                SystemPropertyKey::DependsOn.uuid(),
                Some(serde_json::Value::Null)
            ),
            (
                SystemPropertyKey::Effort.uuid(),
                Some(serde_json::Value::Null)
            ),
            (
                SystemPropertyKey::StoryPoints.uuid(),
                Some(serde_json::Value::Null)
            ),
            (
                SystemPropertyKey::RelevantDocuments.uuid(),
                Some(serde_json::Value::Null),
            ),
            (
                SystemPropertyKey::Milestone.uuid(),
                Some(serde_json::Value::Null)
            ),
            (
                SystemPropertyKey::StartDate.uuid(),
                Some(serde_json::Value::Null)
            ),
        ]
    );

    Ok(())
}

// ============================================================================
// copy_task_properties tests
// ============================================================================

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn test_copy_task_properties_empty_source(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());

    let from_task_id = "source-task-123";
    let to_task_id = "dest-task-456";

    // Copy from empty source - should still create system properties with null values
    repo.copy_task_properties(from_task_id, to_task_id).await?;

    // Should have 12 system properties with null values.
    let count = count_task_properties(&pool, to_task_id).await;
    assert_eq!(count, 12, "Should have 12 system task properties");

    // All values should be null
    let properties = get_task_property_values(&pool, to_task_id).await;
    for (_, value) in &properties {
        assert!(value.is_none(), "All properties should be null");
    }

    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("system_properties"))
)]
async fn test_copy_task_properties_with_existing_properties(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());

    let from_task_id = "source-task-with-props";
    let to_task_id = "dest-task-new";

    // Copy properties (source has 2 system + 2 custom properties from fixture)
    repo.copy_task_properties(from_task_id, to_task_id).await?;

    // Destination should have 14 properties:
    // - 3 copied (Priority, Custom Notes, Custom Tags)
    // - 11 null system properties backfilled, including Status
    let count = count_task_properties(&pool, to_task_id).await;
    assert_eq!(
        count, 14,
        "Should have 14 properties (3 copied + 11 backfilled)"
    );

    // A copied task starts from the canonical unset/Not Started state instead
    // of bypassing the guarded Status mutation path.
    let properties = get_task_property_values(&pool, to_task_id).await;

    let status_prop = properties
        .iter()
        .find(|(id, _)| *id == SystemPropertyKey::Status.uuid());
    assert!(status_prop.is_some(), "Status property should exist");
    assert!(
        status_prop.unwrap().1.is_none(),
        "Status value should reset"
    );

    let priority_prop = properties
        .iter()
        .find(|(id, _)| *id == SystemPropertyKey::Priority.uuid());
    assert!(priority_prop.is_some(), "Priority property should exist");
    assert_eq!(
        priority_prop.unwrap().1.as_ref().unwrap(),
        &serde_json::json!({"type": "SelectOption", "value": ["00000001-0000-0000-0003-000000000003"]}), // High
        "Priority value should be copied"
    );

    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("system_properties"))
)]
async fn test_copy_task_properties_copies_custom_properties(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());

    let from_task_id = "source-task-with-props";
    let to_task_id = "dest-task-custom";

    // Custom property IDs from fixture
    let custom_notes_id: Uuid = "cccccccc-cccc-cccc-cccc-cccccccccc01".parse().unwrap();
    let custom_tags_id: Uuid = "cccccccc-cccc-cccc-cccc-cccccccccc02".parse().unwrap();

    repo.copy_task_properties(from_task_id, to_task_id).await?;

    let properties = get_task_property_values(&pool, to_task_id).await;

    // Check Custom Notes was copied
    let notes_prop = properties.iter().find(|(id, _)| *id == custom_notes_id);
    assert!(
        notes_prop.is_some(),
        "Custom Notes property should be copied"
    );
    assert_eq!(
        notes_prop.unwrap().1.as_ref().unwrap(),
        &serde_json::json!({"type": "String", "value": "This is a custom note"}),
        "Custom Notes value should be copied"
    );

    // Check Custom Tags was copied
    let tags_prop = properties.iter().find(|(id, _)| *id == custom_tags_id);
    assert!(tags_prop.is_some(), "Custom Tags property should be copied");
    assert_eq!(
        tags_prop.unwrap().1.as_ref().unwrap(),
        &serde_json::json!({"type": "SelectOption", "value": ["00000000-0000-0000-0000-000000000101"]}), // urgent
        "Custom Tags value should be copied"
    );

    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(path = "../../../fixtures", scripts("system_properties"))
)]
async fn test_copy_task_properties_preserves_guarded_existing_status(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());

    let from_task_id = "source-task-overwrite"; // Status = Completed
    let to_task_id = "dest-task-existing"; // Status = Not Started

    let retained_dependency = serde_json::json!({
        "type": "EntityReference",
        "value": [{"entity_id": "unfinished-predecessor", "entity_type": "TASK"}]
    });
    sqlx::query(
        "INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(to_task_id)
    .bind(SystemPropertyKey::DependsOn.uuid())
    .bind(retained_dependency.clone())
    .execute(&pool)
    .await?;

    // Copy must not write Completed around the common TASKDEPS guard. The
    // destination keeps both its prior status and dependency graph.
    repo.copy_task_properties(from_task_id, to_task_id).await?;

    let properties = get_task_property_values(&pool, to_task_id).await;
    let status_prop = properties
        .iter()
        .find(|(id, _)| *id == SystemPropertyKey::Status.uuid());

    assert_eq!(
        status_prop.unwrap().1.as_ref().unwrap(),
        &serde_json::json!({"type": "SelectOption", "value": ["00000001-0000-0000-0002-000000000001"]}), // Not Started (destination)
        "Guarded destination Status must not be overwritten"
    );
    assert_eq!(
        properties
            .iter()
            .find(|(id, _)| *id == SystemPropertyKey::DependsOn.uuid())
            .expect("Depends On remains attached")
            .1,
        Some(retained_dependency)
    );

    Ok(())
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn test_copy_task_properties_idempotent(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());

    let from_task_id = "source-task-idempotent";
    let to_task_id = "dest-task-idempotent";

    // Copy twice
    repo.copy_task_properties(from_task_id, to_task_id).await?;
    repo.copy_task_properties(from_task_id, to_task_id).await?;

    // Should still have exactly 12 properties.
    let count = count_task_properties(&pool, to_task_id).await;
    assert_eq!(
        count, 12,
        "Should have exactly 12 properties after idempotent copies"
    );

    Ok(())
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn test_copy_task_properties_preserves_milestone_boolean_values(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());
    let milestone_id = SystemPropertyKey::Milestone.uuid();

    for (source, destination, milestone) in [
        (
            "source-task-milestone-true",
            "destination-task-milestone-true",
            true,
        ),
        (
            "source-task-milestone-false",
            "destination-task-milestone-false",
            false,
        ),
    ] {
        sqlx::query(
            "INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(source)
        .bind(milestone_id)
        .bind(serde_json::json!({"type": "Boolean", "value": milestone}))
        .execute(&pool)
        .await?;

        repo.copy_task_properties(source, destination).await?;

        assert_eq!(count_task_properties(&pool, destination).await, 12);
        assert_eq!(
            get_task_property_values(&pool, destination)
                .await
                .into_iter()
                .find(|(id, _)| *id == milestone_id)
                .expect("Milestone property is attached to copied task")
                .1,
            Some(serde_json::json!({"type": "Boolean", "value": milestone}))
        );
    }

    Ok(())
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn test_copy_task_properties_preserves_start_date_and_null(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());
    let start_date_id = SystemPropertyKey::StartDate.uuid();
    let start_date = serde_json::json!({
        "type": "Date",
        "value": "2026-09-01T12:34:56Z"
    });

    sqlx::query(
        "INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4), ($5, $6, 'TASK', $3, NULL)",
    )
    .bind(Uuid::new_v4())
    .bind("source-task-start-date")
    .bind(start_date_id)
    .bind(start_date.clone())
    .bind(Uuid::new_v4())
    .bind("source-task-start-date-null")
    .execute(&pool)
    .await?;

    repo.copy_task_properties("source-task-start-date", "destination-task-start-date")
        .await?;
    repo.copy_task_properties(
        "source-task-start-date-null",
        "destination-task-start-date-null",
    )
    .await?;

    for (destination, expected) in [
        ("destination-task-start-date", Some(start_date)),
        (
            "destination-task-start-date-null",
            Some(serde_json::Value::Null),
        ),
    ] {
        assert_eq!(count_task_properties(&pool, destination).await, 12);
        assert_eq!(
            get_task_property_values(&pool, destination)
                .await
                .into_iter()
                .find(|(id, _)| *id == start_date_id)
                .expect("Start Date property is attached to copied task")
                .1,
            expected
        );
    }

    Ok(())
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn decision_attach_is_bounded_and_does_not_overwrite_existing_state(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let service = SystemPropertiesServiceImpl::new(PgSystemPropertiesRepository::new(pool.clone()));
    let decision_id = "decision-attach-idempotent";

    service
        .attach_decision_properties(vec![decision_id.to_owned()])
        .await?;
    assert_eq!(
        sqlx::query_scalar::<_, Option<serde_json::Value>>(
            "SELECT values FROM entity_properties WHERE entity_id = $1 AND entity_type = 'DOCUMENT' AND property_definition_id = $2",
        )
        .bind(decision_id)
        .bind(SystemPropertyKey::DECISION_STATE_UUID)
        .fetch_one(&pool)
        .await?,
        Some(serde_json::json!({
            "type": "SelectOption",
            "value": [DecisionStateOption::PROPOSED_UUID]
        }))
    );
    sqlx::query(
        "UPDATE entity_properties SET values = $1 WHERE entity_id = $2 AND entity_type = 'DOCUMENT' AND property_definition_id = $3",
    )
    .bind(serde_json::json!({
        "type": "SelectOption",
        "value": [DecisionStateOption::ACCEPTED_UUID]
    }))
    .bind(decision_id)
    .bind(SystemPropertyKey::DECISION_STATE_UUID)
    .execute(&pool)
    .await?;

    service
        .attach_decision_properties(vec![decision_id.to_owned()])
        .await?;

    let rows = sqlx::query_as::<_, (Uuid, Option<serde_json::Value>)>(
        "SELECT property_definition_id, values FROM entity_properties WHERE entity_id = $1 AND entity_type = 'DOCUMENT' ORDER BY property_definition_id",
    )
    .bind(decision_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter()
            .find(|(id, _)| *id == SystemPropertyKey::DECISION_STATE_UUID)
            .and_then(|(_, value)| value.clone()),
        Some(serde_json::json!({
            "type": "SelectOption",
            "value": [DecisionStateOption::ACCEPTED_UUID]
        }))
    );
    assert!(rows.iter().all(|(id, _)| {
        [
            SystemPropertyKey::DECISION_STATE_UUID,
            SystemPropertyKey::DECIDED_BY_UUID,
            SystemPropertyKey::DECIDED_AT_UUID,
            SystemPropertyKey::DECISION_SOURCES_UUID,
        ]
        .contains(id)
    }));

    Ok(())
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn decision_copy_preserves_only_decision_values(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let service = SystemPropertiesServiceImpl::new(PgSystemPropertiesRepository::new(pool.clone()));
    let source = "decision-copy-source";
    let destination = "decision-copy-destination";
    let values = [
        (
            SystemPropertyKey::DECISION_STATE_UUID,
            serde_json::json!({
                "type": "SelectOption",
                "value": [DecisionStateOption::SUPERSEDED_UUID]
            }),
        ),
        (
            SystemPropertyKey::DECIDED_BY_UUID,
            serde_json::json!({
                "type": "EntityReference",
                "value": [{"entity_type": "USER", "entity_id": "macro|decider@example.test"}]
            }),
        ),
        (
            SystemPropertyKey::DECIDED_AT_UUID,
            serde_json::json!({"type": "Date", "value": "2026-09-03T03:00:00Z"}),
        ),
        (
            SystemPropertyKey::DECISION_SOURCES_UUID,
            serde_json::json!({
                "type": "Link",
                "value": ["https://example.test/source-a", "https://example.test/source-b"]
            }),
        ),
    ];

    service
        .attach_decision_properties(vec![source.to_owned()])
        .await?;
    for (property_id, value) in &values {
        sqlx::query(
            "UPDATE entity_properties SET values = $1 WHERE entity_id = $2 AND entity_type = 'DOCUMENT' AND property_definition_id = $3",
        )
        .bind(value)
        .bind(source)
        .bind(property_id)
        .execute(&pool)
        .await?;
    }
    sqlx::query(
        "INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'DOCUMENT', $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(source)
    .bind(SystemPropertyKey::SUBJECT_UUID)
    .bind(serde_json::json!({"type": "String", "value": "not a Decision property"}))
    .execute(&pool)
    .await?;

    service
        .copy_decision_properties(source, destination)
        .await?;

    let copied = sqlx::query_as::<_, (Uuid, Option<serde_json::Value>)>(
        "SELECT property_definition_id, values FROM entity_properties WHERE entity_id = $1 AND entity_type = 'DOCUMENT' ORDER BY property_definition_id",
    )
    .bind(destination)
    .fetch_all(&pool)
    .await?;
    assert_eq!(copied.len(), 4);
    for (property_id, expected) in values {
        assert_eq!(
            copied
                .iter()
                .find(|(id, _)| *id == property_id)
                .and_then(|(_, value)| value.clone()),
            Some(expected)
        );
    }
    assert!(
        !copied
            .iter()
            .any(|(id, _)| *id == SystemPropertyKey::SUBJECT_UUID)
    );

    Ok(())
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn test_copy_task_properties_does_not_copy_hierarchy_or_depends_on(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());
    let source = "source-task-dependencies";
    let absent_destination = "destination-task-dependencies-absent";
    let existing_destination = "destination-task-dependencies-existing";
    let source_value = serde_json::json!({
        "type": "EntityReference",
        "value": [{"entity_id": "other-task", "entity_type": "TASK"}]
    });
    let destination_value = serde_json::json!({
        "type": "EntityReference",
        "value": [{"entity_id": "retained-task", "entity_type": "TASK"}]
    });

    for (task_id, value) in [
        (source, source_value),
        (existing_destination, destination_value),
    ] {
        sqlx::query(
            "INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(task_id)
        .bind(SystemPropertyKey::DependsOn.uuid())
        .bind(value)
        .execute(&pool)
        .await?;
    }
    for (task_id, reference_id) in [
        (source, "source-hierarchy-task"),
        (existing_destination, "retained-hierarchy-task"),
    ] {
        for property_id in [
            SystemPropertyKey::ParentTask.uuid(),
            SystemPropertyKey::Subtasks.uuid(),
        ] {
            sqlx::query(
                "INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)",
            )
            .bind(Uuid::new_v4())
            .bind(task_id)
            .bind(property_id)
            .bind(serde_json::json!({"type": "EntityReference", "value": [{"entity_id": reference_id, "entity_type": "TASK"}]}))
            .execute(&pool)
            .await?;
        }
    }
    let status_value = serde_json::json!({"type": "SelectOption", "value": []});
    sqlx::query(
        "INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(source)
    .bind(SystemPropertyKey::Status.uuid())
    .bind(status_value.clone())
    .execute(&pool)
    .await?;

    repo.copy_task_properties(source, absent_destination)
        .await?;
    repo.copy_task_properties(source, existing_destination)
        .await?;

    let absent_value = get_task_property_values(&pool, absent_destination)
        .await
        .into_iter()
        .find(|(id, _)| *id == SystemPropertyKey::DependsOn.uuid())
        .unwrap()
        .1;
    assert!(absent_value.is_none());
    let retained_value = get_task_property_values(&pool, existing_destination)
        .await
        .into_iter()
        .find(|(id, _)| *id == SystemPropertyKey::DependsOn.uuid())
        .unwrap()
        .1;
    assert_eq!(
        retained_value,
        Some(serde_json::json!({
            "type": "EntityReference",
            "value": [{"entity_id": "retained-task", "entity_type": "TASK"}]
        }))
    );
    for property_id in [
        SystemPropertyKey::ParentTask.uuid(),
        SystemPropertyKey::Subtasks.uuid(),
    ] {
        let absent = get_task_property_values(&pool, absent_destination)
            .await
            .into_iter()
            .find(|(id, _)| *id == property_id)
            .unwrap()
            .1;
        assert!(absent.is_none());
        let retained = get_task_property_values(&pool, existing_destination)
            .await
            .into_iter()
            .find(|(id, _)| *id == property_id)
            .unwrap()
            .1;
        assert_eq!(
            retained,
            Some(
                serde_json::json!({"type": "EntityReference", "value": [{"entity_id": "retained-hierarchy-task", "entity_type": "TASK"}]})
            )
        );
    }
    assert_eq!(
        get_task_property_values(&pool, absent_destination)
            .await
            .into_iter()
            .find(|(id, _)| *id == SystemPropertyKey::Status.uuid())
            .unwrap()
            .1,
        None,
        "Copied task Status must start unset instead of bypassing the guard"
    );
    assert_eq!(count_task_properties(&pool, absent_destination).await, 12);
    assert_eq!(count_task_properties(&pool, existing_destination).await, 12);
    Ok(())
}
