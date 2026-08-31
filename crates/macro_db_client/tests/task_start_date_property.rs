use sqlx::{Pool, Postgres};
use uuid::Uuid;

const START_DATE_ID: &str = "00000001-0000-0000-0000-000000000014";
const STATUS_ID: &str = "00000001-0000-0000-0000-000000000002";
const MIGRATION: &str = include_str!("../migrations/20260831120000_task_start_date_property.sql");

#[sqlx::test]
async fn task_start_date_migration_backfills_only_task_documents_and_reapplies_by_cascade(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let start_date_id: Uuid = START_DATE_ID.parse()?;
    let status_id: Uuid = STATUS_ID.parse()?;
    let owner = "task-start-date-migration-owner";
    let owner_macro_user_id = Uuid::from_u128(0xD600_0000_0000_0000_0000_0000_0000_0001);
    let live_task = "task-start-date-migration-live";
    let deleted_task = "task-start-date-migration-deleted";
    let non_task_document = "task-start-date-migration-document";

    sqlx::query("DELETE FROM property_definitions WHERE id = $1")
        .bind(start_date_id)
        .execute(&pool)
        .await?;

    sqlx::query(
        "INSERT INTO macro_user (id, username, email, stripe_customer_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(owner_macro_user_id)
    .bind(owner)
    .bind("task-start-date-migration-owner@example.test")
    .bind("cus_task_start_date_migration_owner")
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO \"User\" (id, email, macro_user_id) VALUES ($1, $2, $3)")
        .bind(owner)
        .bind("task-start-date-migration-owner@example.test")
        .bind(owner_macro_user_id)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO \"Document\" (id, name, owner, \"deletedAt\") VALUES ($1, $2, $3, $4), ($5, $6, $3, $7), ($8, $9, $3, NULL)",
    )
    .bind(live_task)
    .bind("live task")
    .bind(owner)
    .bind(Option::<chrono::DateTime<chrono::Utc>>::None)
    .bind(deleted_task)
    .bind("soft-deleted task")
    .bind(chrono::DateTime::parse_from_rfc3339("2026-08-30T00:00:00Z")?.to_utc())
    .bind(non_task_document)
    .bind("ordinary document")
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task'), ($2, 'task')",
    )
    .bind(live_task)
    .bind(deleted_task)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(live_task)
    .bind(status_id)
    .bind(serde_json::json!({"type": "SelectOption", "value": []}))
    .execute(&pool)
    .await?;

    sqlx::raw_sql(MIGRATION).execute(&pool).await?;

    let definition = sqlx::query_as::<_, (Uuid, String, String, bool, Option<String>, bool, Option<Uuid>, Option<String>)>(
        "SELECT id, display_name, data_type::text, is_multi_select, specific_entity_type::text, is_system, team_id, user_id FROM property_definitions WHERE id = $1",
    )
    .bind(start_date_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        definition,
        (
            start_date_id,
            "Start Date".to_owned(),
            "DATE".to_owned(),
            false,
            None,
            true,
            None,
            None,
        )
    );
    assert_eq!(
        sqlx::query_as::<_, (String, String, Option<serde_json::Value>)>(
            "SELECT entity_id, entity_type::text, values FROM entity_properties WHERE property_definition_id = $1 ORDER BY entity_id",
        )
        .bind(start_date_id)
        .fetch_all(&pool)
        .await?,
        vec![
            (deleted_task.to_owned(), "TASK".to_owned(), None),
            (live_task.to_owned(), "TASK".to_owned(), None),
        ]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2",
        )
        .bind(non_task_document)
        .bind(start_date_id)
        .fetch_one(&pool)
        .await?,
        0
    );

    sqlx::query("DELETE FROM property_definitions WHERE id = $1")
        .bind(start_date_id)
        .execute(&pool)
        .await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM property_definitions WHERE id = $1")
            .bind(start_date_id)
            .fetch_one(&pool)
            .await?,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM entity_properties WHERE property_definition_id = $1"
        )
        .bind(start_date_id)
        .fetch_one(&pool)
        .await?,
        0
    );
    assert_eq!(
        sqlx::query_as::<_, (Uuid, serde_json::Value)>(
            "SELECT property_definition_id, values FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK'",
        )
        .bind(live_task)
        .fetch_all(&pool)
        .await?,
        vec![(
            status_id,
            serde_json::json!({"type": "SelectOption", "value": []}),
        )]
    );

    sqlx::raw_sql(MIGRATION).execute(&pool).await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM entity_properties WHERE property_definition_id = $1",
        )
        .bind(start_date_id)
        .fetch_one(&pool)
        .await?,
        2
    );

    Ok(())
}
