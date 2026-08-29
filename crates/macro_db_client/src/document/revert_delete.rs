use anyhow::Context;
use system_properties::outbound::task_readiness::final_ready_dependents;
use uuid::Uuid;

/// Reverts a document deletion
/// Adds the document back to the users history as well
#[tracing::instrument(skip(db))]
pub async fn revert_delete_document(
    db: &sqlx::Pool<sqlx::Postgres>,
    document_id: &str,
) -> anyhow::Result<Vec<Uuid>> {
    let mut transaction = db.begin().await.context("unable to begin transaction")?;

    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(&mut *transaction)
    .await
    .context("unable to lock task dependencies")?;

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

    let mut ready_project_id = restored.project_id;
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

    Ok(ready_task_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{Pool, Postgres};

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn test_revert_delete_document(pool: Pool<Postgres>) -> anyhow::Result<()> {
        let first_ready_task_ids = revert_delete_document(&pool, "document-one").await?;
        assert_eq!(first_ready_task_ids, Vec::<Uuid>::new());

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

        assert_eq!(first_ready_task_ids, Vec::<Uuid>::new());
        assert_eq!(retry_ready_task_ids, Vec::<Uuid>::new());
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
        assert_eq!(restore.await??, Vec::<Uuid>::new());
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

        revert_delete_document(&pool, "document-one").await?;

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

        revert_delete_document(&pool, "document-one").await?;

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
}
