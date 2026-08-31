//! Tests for system properties PostgreSQL repository.

use models_properties::EntityType;

use super::*;
use crate::domain::model::{PropertyRow, SystemPropertyKey};
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
    // - 4 copied (Status, Priority, Custom Notes, Custom Tags)
    // - 10 null system properties backfilled
    let count = count_task_properties(&pool, to_task_id).await;
    assert_eq!(
        count, 14,
        "Should have 14 properties (4 copied + 10 backfilled)"
    );

    // Check that status was copied with correct SelectOption value
    let properties = get_task_property_values(&pool, to_task_id).await;

    let status_prop = properties
        .iter()
        .find(|(id, _)| *id == SystemPropertyKey::Status.uuid());
    assert!(status_prop.is_some(), "Status property should exist");
    assert_eq!(
        status_prop.unwrap().1.as_ref().unwrap(),
        &serde_json::json!({"type": "SelectOption", "value": ["00000001-0000-0000-0002-000000000002"]}), // In Progress
        "Status value should be copied"
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
async fn test_copy_task_properties_overwrites_existing(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = PgSystemPropertiesRepository::new(pool.clone());

    let from_task_id = "source-task-overwrite"; // Status = Completed
    let to_task_id = "dest-task-existing"; // Status = Not Started

    // Copy should overwrite destination value
    repo.copy_task_properties(from_task_id, to_task_id).await?;

    let properties = get_task_property_values(&pool, to_task_id).await;
    let status_prop = properties
        .iter()
        .find(|(id, _)| *id == SystemPropertyKey::Status.uuid());

    assert_eq!(
        status_prop.unwrap().1.as_ref().unwrap(),
        &serde_json::json!({"type": "SelectOption", "value": ["00000001-0000-0000-0002-000000000004"]}), // Completed (from source)
        "Destination value should be overwritten with source value"
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
        Some(status_value)
    );
    assert_eq!(count_task_properties(&pool, absent_destination).await, 12);
    assert_eq!(count_task_properties(&pool, existing_destination).await, 12);
    Ok(())
}
