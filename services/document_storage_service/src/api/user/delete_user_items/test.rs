use std::time::Duration;

use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::{delete_user_items_in_transaction, document_cleanup_queue};

const DELETED_USER: &str = "macro|user@user.com";
const SURVIVING_USER: &str = "macro|other@user.com";
static MACRO_DB_MIGRATIONS: sqlx::migrate::Migrator =
    sqlx::migrate!("../../crates/macro_db_client/migrations");

fn expected_document_ids() -> Vec<String> {
    [
        "document-deleted",
        "document-five",
        "document-four",
        "document-one",
        "document-seven",
        "document-six",
        "document-three",
        "document-two",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

async fn insert_surviving_user(pool: &PgPool) -> anyhow::Result<()> {
    let other_macro_user_id = Uuid::parse_str("b2222222-2222-2222-2222-222222222222")?;
    sqlx::query(
        r#"
        INSERT INTO macro_user (id, username, email, stripe_customer_id)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(other_macro_user_id)
    .bind("other-user")
    .bind("other@user.com")
    .bind("other-stripe-id")
    .execute(pool)
    .await?;
    sqlx::query(r#"INSERT INTO "User" (id, email, macro_user_id) VALUES ($1, $2, $3)"#)
        .bind(SURVIVING_USER)
        .bind("other@user.com")
        .bind(other_macro_user_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn insert_document(pool: &PgPool, id: &str, owner: &str) -> anyhow::Result<()> {
    sqlx::query(r#"INSERT INTO "Document" (id, name, "fileType", owner, "createdAt", "updatedAt", "projectId") VALUES ($1, $2, $3, $4, NOW(), NOW(), 'project-one')"#)
        .bind(id).bind(id).bind("txt").bind(owner).execute(pool).await?;
    Ok(())
}

async fn mark_task(pool: &PgPool, id: &str) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, 'task')")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn task_property(
    pool: &PgPool,
    entity_id: &str,
    definition: uuid::Uuid,
    values: Value,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
        .bind(Uuid::new_v4()).bind(entity_id).bind(definition).bind(values).execute(pool).await?;
    Ok(())
}

async fn property_value(
    pool: &PgPool,
    entity_id: &str,
    definition: uuid::Uuid,
) -> anyhow::Result<Option<Value>> {
    let row = sqlx::query(
        "SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2",
    )
    .bind(entity_id)
    .bind(definition)
    .fetch_optional(pool)
    .await?;
    Ok(row
        .map(|row| row.try_get::<Option<Value>, _>("values"))
        .transpose()?
        .flatten())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(
        path = "../../../../../../crates/macro_db_client/fixtures",
        scripts("basic_user_with_lots_of_documents")
    )
)]
async fn mixed_owner_hierarchy_cleanup_runs_before_owner_documents_are_removed(
    pool: PgPool,
) -> anyhow::Result<()> {
    let source = "document-one";
    let survivor = "other-user-task";
    let malformed = "other-user-malformed-task";
    let non_task = "other-user-non-task";
    insert_surviving_user(&pool).await?;
    for id in [survivor, malformed, non_task] {
        insert_document(&pool, id, SURVIVING_USER).await?;
    }
    for id in [source, survivor, malformed] {
        mark_task(&pool, id).await?;
    }
    let keep_first =
        json!({"entity_id":malformed,"entity_type":"TASK","specific_message_id":"keep-first"});
    let keep_second = json!({"entity_id":survivor,"entity_type":"TASK","extra":[1,2]});
    task_property(
        &pool,
        source,
        system_properties::SystemPropertyKey::STATUS_UUID,
        json!({"type":"SelectOption","value":[]}),
    )
    .await?;
    task_property(
        &pool,
        source,
        system_properties::SystemPropertyKey::PARENT_TASK_UUID,
        json!({"type":"EntityReference","value":[{"entity_id":survivor,"entity_type":"TASK"}]}),
    )
    .await?;
    task_property(&pool, survivor, system_properties::SystemPropertyKey::SUBTASKS_UUID, json!({"type":"EntityReference","value":[{"entity_id":source,"entity_type":"TASK","extra":"remove"},keep_first.clone(),{"entity_id":source,"entity_type":"TASK"},keep_second.clone()]})).await?;
    task_property(
        &pool,
        survivor,
        system_properties::SystemPropertyKey::PARENT_TASK_UUID,
        json!({"type":"EntityReference","value":[{"entity_id":source,"entity_type":"TASK"}]}),
    )
    .await?;
    task_property(
        &pool,
        malformed,
        system_properties::SystemPropertyKey::PARENT_TASK_UUID,
        json!({"malformed":true}),
    )
    .await?;
    task_property(
        &pool,
        malformed,
        system_properties::SystemPropertyKey::DEPENDS_ON_UUID,
        json!({"type":"EntityReference","value":[{"entity_id":source,"entity_type":"TASK"}]}),
    )
    .await?;
    sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'DOCUMENT', $3, $4)")
        .bind(Uuid::new_v4()).bind(non_task).bind(system_properties::SystemPropertyKey::STATUS_UUID).bind(json!({"preserve":"non-task"})).execute(&pool).await?;

    let mut transaction = pool.begin().await?;
    let document_ids = delete_user_items_in_transaction(&mut transaction, DELETED_USER).await?;
    transaction.commit().await?;
    assert_eq!(document_ids, expected_document_ids());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM entity_properties WHERE entity_id = $1 AND entity_type = 'TASK'"
        )
        .bind(source)
        .fetch_one(&pool)
        .await?,
        0
    );
    assert_eq!(
        property_value(
            &pool,
            survivor,
            system_properties::SystemPropertyKey::SUBTASKS_UUID
        )
        .await?,
        Some(json!({"type":"EntityReference","value":[keep_first,keep_second]}))
    );
    assert_eq!(
        property_value(
            &pool,
            survivor,
            system_properties::SystemPropertyKey::PARENT_TASK_UUID
        )
        .await?,
        None
    );
    assert_eq!(
        property_value(
            &pool,
            malformed,
            system_properties::SystemPropertyKey::PARENT_TASK_UUID
        )
        .await?,
        Some(json!({"malformed":true}))
    );
    assert!(
        property_value(
            &pool,
            malformed,
            system_properties::SystemPropertyKey::DEPENDS_ON_UUID
        )
        .await?
        .is_some()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM \"Document\" WHERE id = ANY($1)")
            .bind([survivor, malformed, non_task])
            .fetch_one(&pool)
            .await?,
        3
    );
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM entity_properties WHERE entity_id = $1 AND entity_type = 'DOCUMENT'").bind(non_task).fetch_one(&pool).await?, 1);
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(
        path = "../../../../../../crates/macro_db_client/fixtures",
        scripts("basic_user_with_lots_of_documents")
    )
)]
async fn helper_failure_rolls_back_all_durable_deletes_and_has_no_cleanup_candidate(
    pool: PgPool,
) -> anyhow::Result<()> {
    mark_task(&pool, "document-one").await?;
    task_property(
        &pool,
        "document-one",
        system_properties::SystemPropertyKey::STATUS_UUID,
        json!({"type":"SelectOption","value":[]}),
    )
    .await?;
    sqlx::raw_sql(r#"CREATE FUNCTION fail_user_item_project_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'user item project rollback sentinel'; END; $$"#).execute(&pool).await?;
    sqlx::raw_sql(r#"CREATE TRIGGER fail_user_item_project_delete BEFORE DELETE ON "Project" FOR EACH ROW EXECUTE FUNCTION fail_user_item_project_delete()"#).execute(&pool).await?;
    let mut transaction = pool.begin().await?;
    let result = delete_user_items_in_transaction(&mut transaction, DELETED_USER).await;
    assert!(
        format!("{:#}", result.as_ref().unwrap_err())
            .contains("user item project rollback sentinel")
    );
    assert!(
        result
            .map(|ids| document_cleanup_queue(&ids, DELETED_USER))
            .unwrap_or_default()
            .is_empty()
    );
    drop(transaction);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM \"Document\" WHERE id = $1")
            .bind("document-one")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM \"Chat\" WHERE id = $1")
            .bind("chat-one")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM \"Project\" WHERE id = $1")
            .bind("project-one")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert!(
        property_value(
            &pool,
            "document-one",
            system_properties::SystemPropertyKey::STATUS_UUID
        )
        .await?
        .is_some()
    );
    Ok(())
}

#[sqlx::test(
    migrator = "MACRO_DB_MIGRATIONS",
    fixtures(
        path = "../../../../../../crates/macro_db_client/fixtures",
        scripts("basic_user_with_lots_of_documents")
    )
)]
async fn owner_purge_serializes_behind_taskhier_and_retry_has_no_cleanup(
    pool: PgPool,
) -> anyhow::Result<()> {
    let mut hierarchy_lock = pool.begin().await?;
    sqlx::query_scalar::<_, i32>("SELECT 1 FROM pg_advisory_xact_lock($1)")
        .bind(i64::from_be_bytes(*b"TASKHIER"))
        .fetch_one(&mut *hierarchy_lock)
        .await?;
    let worker_pool = pool.clone();
    let mut worker = tokio::spawn(async move {
        let mut transaction = worker_pool.begin().await?;
        let ids = delete_user_items_in_transaction(&mut transaction, DELETED_USER).await?;
        transaction.commit().await?;
        Ok::<_, anyhow::Error>(ids)
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(25), &mut worker)
            .await
            .is_err()
    );
    hierarchy_lock.commit().await?;
    let first = tokio::time::timeout(Duration::from_secs(1), worker).await???;
    assert_eq!(first, expected_document_ids());
    assert_eq!(
        document_cleanup_queue(&first, DELETED_USER),
        first
            .iter()
            .map(|id| (id.clone(), DELETED_USER.to_owned()))
            .collect::<Vec<_>>()
    );
    let mut retry = pool.begin().await?;
    let second = delete_user_items_in_transaction(&mut retry, DELETED_USER).await?;
    retry.commit().await?;
    assert!(second.is_empty());
    assert!(document_cleanup_queue(&second, DELETED_USER).is_empty());
    Ok(())
}

#[test]
fn post_commit_cleanup_preserves_the_classified_document_order_and_owner() {
    let document_ids = vec!["document-a".to_owned(), "document-b".to_owned()];
    assert_eq!(
        document_cleanup_queue(&document_ids, "macro|deleted@example.com"),
        vec![
            (
                "document-a".to_owned(),
                "macro|deleted@example.com".to_owned()
            ),
            (
                "document-b".to_owned(),
                "macro|deleted@example.com".to_owned()
            )
        ]
    );
}
