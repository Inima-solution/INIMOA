use anyhow::Context;
use system_properties::outbound::task_readiness::final_ready_dependents;
use uuid::Uuid;

/// Result of one committed direct document restore.
#[derive(Debug, PartialEq, Eq)]
pub struct DocumentRestoreOutcome {
    pub ready_task_ids: Vec<Uuid>,
    pub project_id: Option<String>,
}

/// Reverts a document deletion
/// Adds the document back to the users history as well
#[tracing::instrument(skip(db))]
pub async fn revert_delete_document(
    db: &sqlx::Pool<sqlx::Postgres>,
    document_id: &str,
) -> anyhow::Result<DocumentRestoreOutcome> {
    let mut transaction = db.begin().await.context("unable to begin transaction")?;

    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(&mut *transaction)
    .await
    .context("unable to lock task dependencies")?;

    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKHIER")
    )
    .fetch_one(&mut *transaction)
    .await
    .context("unable to lock task hierarchy")?;

    // Capture whether this call made the document available.  A retry still
    // refreshes history, but it must never fan out a second readiness event.
    let restored = sqlx::query!(
        r#"
        WITH previous AS (
            SELECT owner, "deletedAt", "projectId"
            FROM "Document"
            WHERE id = $1
            FOR UPDATE
        )
        UPDATE "Document" document
        SET "deletedAt" = NULL
        FROM previous
        WHERE document.id = $1
        RETURNING document.owner AS owner,
            previous."projectId" AS project_id,
            previous."deletedAt" IS NOT NULL AS "actually_restored!"
        "#,
        document_id,
    )
    .fetch_one(&mut *transaction)
    .await
    .context("unable to update document")?;
    let document_owner = restored.owner;
    let actually_restored = restored.actually_restored;

    // Add document back to history
    sqlx::query!(
        r#"
        INSERT INTO "UserHistory" ("userId", "itemId", "itemType", "createdAt", "updatedAt")
        VALUES ($1, $2, $3, NOW(), NOW())
        ON CONFLICT ("userId", "itemId", "itemType") DO UPDATE
        SET "updatedAt" = NOW();
        "#,
        document_owner,
        document_id,
        "document",
    )
    .execute(&mut *transaction)
    .await
    .context("unable to add document to history")?;

    let previous_project_id = restored.project_id;
    let mut ready_project_id = previous_project_id.clone();
    if let Some(project_id) = ready_project_id.as_deref() {
        tracing::trace!("document was in nested");
        let is_deleted = sqlx::query!(
            r#"
            SELECT "deletedAt" as deleted_at FROM "Project" WHERE "id" = $1
            "#,
            project_id
        )
        .map(|row| row.deleted_at)
        .fetch_one(&mut *transaction)
        .await?;

        if is_deleted.is_some() {
            tracing::trace!("project is deleted, removing document from project");

            sqlx::query!(
                r#"
                UPDATE "Document" SET "projectId" = NULL WHERE "id" = $1
                "#,
                document_id
            )
            .execute(&mut *transaction)
            .await?;
            ready_project_id = None;
        }
    }

    if actually_restored && ready_project_id != previous_project_id {
        system_properties::outbound::task_hierarchy_lifecycle::reconcile_relocated_task_hierarchy(
            &mut transaction,
            document_id,
        )
        .await?;
    }

    let ready_task_ids = if actually_restored {
        if let Ok(document_id) = Uuid::parse_str(document_id) {
            final_ready_dependents(
                &mut transaction,
                &[document_id],
                ready_project_id.as_deref(),
                &[document_id],
            )
            .await?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    transaction
        .commit()
        .await
        .context("unable to commit transaction")?;

    Ok(DocumentRestoreOutcome {
        ready_task_ids,
        project_id: ready_project_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{Pool, Postgres, Row};

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn test_revert_delete_document(pool: Pool<Postgres>) -> anyhow::Result<()> {
        let first = revert_delete_document(&pool, "document-one").await?;
        assert_eq!(first.ready_task_ids, Vec::<Uuid>::new());

        let document = sqlx::query!(
            r#"
            SELECT "deletedAt" as deleted_at FROM "Document" WHERE id = $1
            "#,
            "document-one"
        )
        .map(|row| row.deleted_at)
        .fetch_one(&pool)
        .await?;

        assert!(document.is_none());

        let _history = sqlx::query!(
            r#"
            SELECT "createdAt" as created_at, "updatedAt" as updated_at FROM "UserHistory" WHERE "userId" = $1 AND "itemId" = $2
            "#,
            "macro|user@user.com",
            "document-one"
        )
        .fetch_one(&pool)
        .await?;

        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn retry_restore_has_no_ready_fanout(pool: Pool<Postgres>) -> anyhow::Result<()> {
        let first_ready_task_ids = revert_delete_document(&pool, "document-one").await?;
        let retry_ready_task_ids = revert_delete_document(&pool, "document-one").await?;

        assert_eq!(first_ready_task_ids.ready_task_ids, Vec::<Uuid>::new());
        assert_eq!(retry_ready_task_ids.ready_task_ids, Vec::<Uuid>::new());
        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn restore_serializes_behind_taskdeps_lifecycle_lock(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        let mut lock_transaction = pool.begin().await?;
        sqlx::query_scalar!(
            r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
            i64::from_be_bytes(*b"TASKDEPS")
        )
        .fetch_one(&mut *lock_transaction)
        .await?;

        let restore_pool = pool.clone();
        let mut restore =
            tokio::spawn(
                async move { revert_delete_document(&restore_pool, "document-one").await },
            );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut restore)
                .await
                .is_err()
        );

        lock_transaction.commit().await?;
        assert_eq!(restore.await??.ready_task_ids, Vec::<Uuid>::new());
        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn restore_serializes_behind_taskhier_lifecycle_lock(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        let mut lock_transaction = pool.begin().await?;
        sqlx::query_scalar!(
            r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
            i64::from_be_bytes(*b"TASKHIER")
        )
        .fetch_one(&mut *lock_transaction)
        .await?;
        let restore_pool = pool.clone();
        let mut restore =
            tokio::spawn(
                async move { revert_delete_document(&restore_pool, "document-one").await },
            );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut restore)
                .await
                .is_err()
        );
        lock_transaction.commit().await?;
        assert_eq!(restore.await??.ready_task_ids, Vec::<Uuid>::new());
        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn test_revert_delete_document_nested_deleted_parent(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        // insert new parent for document-one that is deleted
        sqlx::query!(
            r#"
            INSERT INTO "Project" ("id", "name", "userId", "deletedAt")
            VALUES ('p1', 'd', 'macro|user@user.com', '2019-10-16 00:00:00')
            "#
        )
        .execute(&pool)
        .await?;

        // update document-one to have parent p1
        sqlx::query!(
            r#"
            UPDATE "Document" SET "projectId" = 'p1' WHERE "id" = 'document-one'
            "#
        )
        .execute(&pool)
        .await?;

        let outcome = revert_delete_document(&pool, "document-one").await?;
        assert_eq!(outcome.project_id, None);

        let project_id = sqlx::query!(
            r#"
            SELECT "projectId" as project_id FROM "Document" WHERE "id" = 'document-one'
            "#
        )
        .map(|row| row.project_id)
        .fetch_one(&pool)
        .await?;

        assert!(project_id.is_none());

        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn test_revert_delete_document_nested_not_deleted_parent(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        // insert new parent for document-one that is deleted
        sqlx::query!(
            r#"
            INSERT INTO "Project" ("id", "name", "userId")
            VALUES ('p1', 'd', 'macro|user@user.com')
            "#
        )
        .execute(&pool)
        .await?;

        // update document-one to have parent p1
        sqlx::query!(
            r#"
            UPDATE "Document" SET "projectId" = 'p1' WHERE "id" = 'document-one'
            "#
        )
        .execute(&pool)
        .await?;

        let outcome = revert_delete_document(&pool, "document-one").await?;
        assert_eq!(outcome.project_id.as_deref(), Some("p1"));

        let project_id = sqlx::query!(
            r#"
            SELECT "projectId" as project_id FROM "Document" WHERE "id" = 'document-one'
            "#
        )
        .map(|row| row.project_id)
        .fetch_one(&pool)
        .await?;

        assert!(project_id.is_some());

        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn uuid_task_deleted_parent_restore_reconciles_hierarchy_once(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        let source = Uuid::new_v4();
        let peer = Uuid::new_v4();
        let other = Uuid::new_v4();
        sqlx::query("INSERT INTO \"Project\" (id, name, \"userId\", \"deletedAt\") VALUES ('deleted-parent', 'p', 'macro|user@user.com', NOW())").execute(&pool).await?;
        for (id, deleted) in [(source, true), (peer, false)] {
            sqlx::query("INSERT INTO \"Document\" (id, name, \"fileType\", owner, \"projectId\", \"deletedAt\") VALUES ($1, 't', 'pdf', 'macro|user@user.com', 'deleted-parent', CASE WHEN $2 THEN NOW() ELSE NULL END)")
                .bind(id.to_string()).bind(deleted).execute(&pool).await?;
            sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, $2::document_sub_type_value)")
                .bind(id.to_string()).bind("task").execute(&pool).await?;
        }
        let parent = system_properties::SystemPropertyKey::PARENT_TASK_UUID;
        let subtasks = system_properties::SystemPropertyKey::SUBTASKS_UUID;
        let survivor = serde_json::json!({"entity_id": other, "entity_type":"TASK", "specific_message_id":"keep"});
        for (id, definition, value) in [
            (
                source,
                parent,
                serde_json::json!({"type":"EntityReference","value":[{"entity_id":peer,"entity_type":"TASK"}]}),
            ),
            (
                source,
                subtasks,
                serde_json::json!({"type":"EntityReference","value":[{"entity_id":peer,"entity_type":"TASK"}]}),
            ),
            (
                peer,
                parent,
                serde_json::json!({"type":"EntityReference","value":[{"entity_id":source,"entity_type":"TASK"},survivor.clone()]}),
            ),
            (
                peer,
                subtasks,
                serde_json::json!({"type":"EntityReference","value":[survivor.clone(),{"entity_id":source,"entity_type":"TASK"}]}),
            ),
        ] {
            sqlx::query("INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values) VALUES ($1, $2, 'TASK', $3, $4)")
                .bind(Uuid::new_v4()).bind(id.to_string()).bind(definition).bind(value).execute(&pool).await?;
        }
        let first = revert_delete_document(&pool, &source.to_string()).await?;
        assert_eq!(first.project_id, None);
        assert!(first.ready_task_ids.is_empty());
        for definition in [parent, subtasks] {
            let source_value: Option<serde_json::Value> = sqlx::query_scalar("SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2").bind(source.to_string()).bind(definition).fetch_one(&pool).await?;
            assert_eq!(source_value, None);
        }
        let peer_parent: Option<serde_json::Value> = sqlx::query_scalar("SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2").bind(peer.to_string()).bind(parent).fetch_one(&pool).await?;
        let peer_subtasks: Option<serde_json::Value> = sqlx::query_scalar("SELECT values FROM entity_properties WHERE entity_id = $1 AND property_definition_id = $2").bind(peer.to_string()).bind(subtasks).fetch_one(&pool).await?;
        assert_eq!(
            peer_parent,
            Some(serde_json::json!({"type":"EntityReference","value":[survivor.clone()]}))
        );
        assert_eq!(
            peer_subtasks,
            Some(serde_json::json!({"type":"EntityReference","value":[survivor]}))
        );
        assert_eq!(
            revert_delete_document(&pool, &source.to_string())
                .await?
                .ready_task_ids,
            Vec::<Uuid>::new()
        );
        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn reconciliation_failure_rolls_back_restore_history_and_relocation(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        let source = Uuid::new_v4();
        sqlx::query("INSERT INTO \"Project\" (id, name, \"userId\", \"deletedAt\") VALUES ('rollback-parent', 'p', 'macro|user@user.com', NOW())").execute(&pool).await?;
        sqlx::query("INSERT INTO \"Document\" (id, name, \"fileType\", owner, \"projectId\", \"deletedAt\") VALUES ($1, 't', 'pdf', 'macro|user@user.com', 'rollback-parent', NOW())").bind(source.to_string()).execute(&pool).await?;
        sqlx::query("INSERT INTO document_sub_type (document_id, sub_type) VALUES ($1, $2::document_sub_type_value)").bind(source.to_string()).bind("task").execute(&pool).await?;
        sqlx::query("DROP TABLE entity_properties")
            .execute(&pool)
            .await?;
        assert!(
            revert_delete_document(&pool, &source.to_string())
                .await
                .is_err()
        );
        let row = sqlx::query(
            "SELECT \"deletedAt\"::timestamptz, \"projectId\" FROM \"Document\" WHERE id = $1",
        )
        .bind(source.to_string())
        .fetch_one(&pool)
        .await?;
        let deleted_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get(0)?;
        let project_id: Option<String> = row.try_get(1)?;
        assert!(deleted_at.is_some());
        assert_eq!(project_id.as_deref(), Some("rollback-parent"));
        let history: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM \"UserHistory\" WHERE \"itemId\" = $1")
                .bind(source.to_string())
                .fetch_one(&pool)
                .await?;
        assert_eq!(history, 0);
        Ok(())
    }
}
