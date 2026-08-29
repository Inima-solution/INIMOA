use model_entity::EntityType;
use sqlx::{Pool, Postgres, Transaction};

/// Data captured while the deleted document row is locked, before its database
/// records are removed. Callers must perform external cleanup only after this
/// function returns [`DocumentPurgeOutcome::Purged`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPurgeMetadata {
    pub document_id: String,
    pub owner: String,
    pub project_id: Option<String>,
    pub file_type: Option<String>,
    pub bom_shas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentPurgeOutcome {
    Purged(DocumentPurgeMetadata),
    StaleOrUnavailable,
}

/// Atomically hard-purges a document only when its soft-delete timestamp still
/// exactly matches the candidate token. This deliberately contains no external
/// side effects; the returned metadata is the post-commit cleanup contract.
#[tracing::instrument(skip(db))]
pub async fn purge_deleted_document(
    db: &Pool<Postgres>,
    document_id: &str,
    deleted_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<DocumentPurgeOutcome> {
    let document_uuid = macro_uuid::string_to_uuid(document_id)
        .map_err(|_| anyhow::anyhow!("invalid document id for purge"))?;
    let mut transaction = db.begin().await?;

    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(&mut *transaction)
    .await?;

    let document = sqlx::query!(
        r#"
        SELECT id, owner, "projectId" AS project_id, "fileType" AS file_type
        FROM "Document"
        WHERE id = $1 AND "deletedAt" = $2
        FOR UPDATE
        "#,
        document_id,
        deleted_at.naive_utc(),
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(document) = document else {
        transaction.commit().await?;
        return Ok(DocumentPurgeOutcome::StaleOrUnavailable);
    };

    let bom_shas = sqlx::query!(
        r#"
        SELECT bp.sha
        FROM "BomPart" bp
        JOIN "DocumentBom" db ON bp."documentBomId" = db.id
        WHERE db."documentId" = $1
        "#,
        document_id,
    )
    .map(|row| row.sha)
    .fetch_all(&mut *transaction)
    .await?;

    delete_document_rows(&mut transaction, document_id, &document_uuid).await?;
    transaction.commit().await?;

    Ok(DocumentPurgeOutcome::Purged(DocumentPurgeMetadata {
        document_id: document.id,
        owner: document.owner,
        project_id: document.project_id,
        file_type: document.file_type,
        bom_shas,
    }))
}

async fn delete_document_rows(
    transaction: &mut Transaction<'_, Postgres>,
    document_id: &str,
    document_uuid: &uuid::Uuid,
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM "Pin" WHERE "pinnedItemId" = $1 AND "pinnedItemType" = $2
        "#,
        document_id,
        "document"
    )
    .execute(transaction.as_mut())
    .await?;
    sqlx::query!(
        r#"
        DELETE FROM "UserHistory" WHERE "itemId" = $1 AND "itemType" = $2
        "#,
        document_id,
        "document"
    )
    .execute(transaction.as_mut())
    .await?;
    let share_permission: Option<String> = sqlx::query!(
        r#"
            SELECT "sharePermissionId" as share_permission_id
            FROM "DocumentPermission"
            WHERE "documentId"=$1"#,
        document_id
    )
    .map(|row| row.share_permission_id)
    .fetch_optional(transaction.as_mut())
    .await?;
    if let Some(share_permission) = share_permission {
        sqlx::query!(
            r#"
            DELETE FROM "SharePermission" WHERE id = $1"#,
            share_permission
        )
        .execute(transaction.as_mut())
        .await?;
    }
    sqlx::query!(r#"DELETE FROM "Document" WHERE id = $1"#, document_id)
        .execute(transaction.as_mut())
        .await?;
    crate::item_access::delete::delete_user_entity_access_by_item(
        transaction,
        document_uuid,
        EntityType::Document,
    )
    .await?;
    Ok(())
}

/// Hard deletes a document from the database.
/// Removing the history and pins for the document as well.
#[tracing::instrument(skip(db))]
pub async fn delete_document(db: &Pool<Postgres>, document_id: &str) -> anyhow::Result<()> {
    let mut transaction = db.begin().await?;
    delete_document_rows(
        &mut transaction,
        document_id,
        &macro_uuid::string_to_uuid(document_id).unwrap(),
    )
    .await?;

    if let Err(e) = transaction.commit().await {
        tracing::error!(error=?e, "unable to commit transaction");
        return Err(anyhow::Error::from(e));
    }
    Ok(())
}

/// Hard deletes documents in bulk from the database.
#[tracing::instrument(skip(transaction))]
pub async fn delete_document_bulk_tsx(
    transaction: &mut Transaction<'_, Postgres>,
    document_ids: &[impl ToString + std::fmt::Debug],
) -> anyhow::Result<()> {
    let document_ids: Vec<String> = document_ids.iter().map(|s| s.to_string()).collect();
    // Delete pins
    sqlx::query!(
        r#"
        DELETE FROM "Pin" WHERE "pinnedItemId" = ANY($1) AND "pinnedItemType" = $2
        "#,
        &document_ids,
        "document",
    )
    .execute(transaction.as_mut())
    .await?;

    // Delete from history
    sqlx::query!(
        r#"
        DELETE FROM "UserHistory" WHERE "itemId" = ANY($1) AND "itemType" = $2
        "#,
        &document_ids,
        "document",
    )
    .execute(transaction.as_mut())
    .await?;

    // Delete SharePermissions
    sqlx::query!(
        r#"
        DELETE FROM "SharePermission" sp
        USING "DocumentPermission" dp 
        WHERE dp."sharePermissionId" = sp.id
        AND dp."documentId" = ANY($1)
    "#,
        &document_ids,
    )
    .execute(transaction.as_mut())
    .await?;

    // Delete chats
    sqlx::query!(
        r#"
        DELETE FROM "Document" 
        WHERE id = ANY($1)
        "#,
        &document_ids,
    )
    .execute(transaction.as_mut())
    .await?;

    crate::item_access::delete::delete_user_entity_access_bulk(
        transaction,
        &document_ids
            .iter()
            .filter_map(|p| macro_uuid::string_to_uuid(p).ok())
            .collect::<Vec<uuid::Uuid>>(),
        EntityType::Document,
    )
    .await?;

    Ok(())
}

/// Deletes a document version from the database.
#[tracing::instrument(skip(db))]
pub async fn delete_document_version(
    db: &Pool<Postgres>,
    document_id: &str,
    document_version_id: i64,
    file_type: &str,
) -> anyhow::Result<()> {
    let total_count = sqlx::query!(
        r#"
        SELECT
            (SELECT COUNT(*) FROM "DocumentInstance" WHERE "documentId" = $1) +
            (SELECT COUNT(*) FROM "DocumentBom" WHERE "documentId" = $1) AS total_count
        "#,
        document_id
    )
    .fetch_one(db)
    .await?;

    if let Some(total_count) = total_count.total_count {
        // We need to delete the entire document
        if total_count == 1 {
            tracing::debug!("document total count is 1, deleting entire document");
            return delete_document(db, document_id).await;
        }
    }

    match file_type {
        "docx" => {
            sqlx::query!(
                r#"DELETE FROM "DocumentBom" WHERE id = $2 and "documentId" = $1"#,
                document_id,
                document_version_id
            )
            .execute(db)
            .await?;
        }
        _ => {
            sqlx::query!(
                r#"DELETE FROM "DocumentInstance" WHERE id = $2 and "documentId" = $1"#,
                document_id,
                document_version_id
            )
            .execute(db)
            .await?;
        }
    }

    Ok(())
}

/// Gets all the shas of a given document bom that are to be deleted.
pub async fn get_shas_for_deletion(
    db: Pool<Postgres>,
    document_id: &str,
) -> anyhow::Result<Vec<String>> {
    let result = sqlx::query!(
        r#"
        SELECT bp.sha
        FROM "BomPart" bp
        JOIN "DocumentBom" db ON bp."documentBomId" = db.id
        WHERE db."documentId" = $1
        "#,
        document_id,
    )
    .fetch_all(&db)
    .await
    .map_err(|err| anyhow::Error::msg(format!("unable to fetch shas for deletion: {:?}", err)))?;

    Ok(result.into_iter().map(|s| s.sha).collect::<Vec<String>>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{Pool, Postgres};

    const PURGE_ID: &str = "00000000-0000-0000-0000-000000000123";

    async fn insert_deleted_document(
        pool: &Pool<Postgres>,
        deleted_at: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            r#"INSERT INTO "Document" (id, name, owner, "fileType", "deletedAt") VALUES ($1, 'purge-test', 'macro|user@user.com', 'docx', $2)"#,
            PURGE_ID,
            deleted_at.naive_utc(),
        ).execute(pool).await?;
        Ok(())
    }

    struct PurgeFixture {
        token: chrono::DateTime<chrono::Utc>,
        share_permission_id: String,
    }

    async fn purge_fixture(pool: &Pool<Postgres>) -> anyhow::Result<PurgeFixture> {
        let token = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00.123Z")?
            .with_timezone(&chrono::Utc);
        sqlx::query!(
            r#"
            INSERT INTO "Project" (id, name, "userId")
            VALUES ('purge-project', 'purge project', 'macro|user@user.com')
            "#,
        )
        .execute(pool)
        .await?;
        insert_deleted_document(pool, token).await?;
        sqlx::query!(
            r#"UPDATE "Document" SET "projectId" = 'purge-project' WHERE id = $1"#,
            PURGE_ID
        )
        .execute(pool)
        .await?;
        let bom_id: i64 = sqlx::query_scalar!(
            r#"INSERT INTO "DocumentBom" ("documentId") VALUES ($1) RETURNING id"#,
            PURGE_ID,
        )
        .fetch_one(pool)
        .await?;
        sqlx::query!(
            r#"INSERT INTO "BomPart" ("documentBomId", sha, path) VALUES ($1, $2, $3)"#,
            bom_id,
            "sha-a",
            "a"
        )
        .execute(pool)
        .await?;
        sqlx::query!(
            r#"INSERT INTO "BomPart" ("documentBomId", sha, path) VALUES ($1, $2, $3)"#,
            bom_id,
            "sha-a",
            "b"
        )
        .execute(pool)
        .await?;
        sqlx::query!(
            r#"INSERT INTO "BomPart" ("documentBomId", sha, path) VALUES ($1, $2, $3)"#,
            bom_id,
            "sha-b",
            "c"
        )
        .execute(pool)
        .await?;
        sqlx::query!(r#"INSERT INTO "Pin" ("userId", "pinnedItemId", "pinnedItemType", "pinIndex") VALUES ('macro|user@user.com', $1, 'document', 1)"#, PURGE_ID).execute(pool).await?;
        sqlx::query!(r#"INSERT INTO "UserHistory" ("userId", "itemId", "itemType") VALUES ('macro|user@user.com', $1, 'document')"#, PURGE_ID).execute(pool).await?;
        let share_permission_id: String =
            sqlx::query_scalar!(r#"INSERT INTO "SharePermission" DEFAULT VALUES RETURNING id"#)
                .fetch_one(pool)
                .await?;
        sqlx::query!(r#"INSERT INTO "DocumentPermission" ("documentId", "sharePermissionId") VALUES ($1, $2)"#, PURGE_ID, share_permission_id)
            .execute(pool).await?;
        sqlx::query!(
            r#"
            INSERT INTO entity_access (entity_id, entity_type, source_id, source_type, access_level)
            VALUES ($1::uuid, 'document', 'macro|user@user.com', 'user', 'owner')
            "#,
            uuid::Uuid::parse_str(PURGE_ID)?,
        )
        .execute(pool)
        .await?;
        Ok(PurgeFixture {
            token,
            share_permission_id,
        })
    }

    #[derive(Debug, PartialEq, Eq)]
    struct PurgeSnapshot {
        document_count: i64,
        pin_count: i64,
        history_count: i64,
        permission_count: i64,
        share_permission_count: i64,
        access_count: i64,
        bom_count: i64,
        bom_part_count: i64,
        deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    async fn purge_snapshot(
        pool: &Pool<Postgres>,
        document_id: &str,
        share_id: &str,
    ) -> anyhow::Result<PurgeSnapshot> {
        let row = sqlx::query!(r#"
            SELECT
                (SELECT COUNT(*) FROM "Document" WHERE id = $1) AS "document_count!",
                (SELECT COUNT(*) FROM "Pin" WHERE "pinnedItemId" = $1) AS "pin_count!",
                (SELECT COUNT(*) FROM "UserHistory" WHERE "itemId" = $1) AS "history_count!",
                (SELECT COUNT(*) FROM "DocumentPermission" WHERE "documentId" = $1) AS "permission_count!",
                (SELECT COUNT(*) FROM "SharePermission" WHERE id = $2) AS "share_permission_count!",
                (SELECT COUNT(*) FROM entity_access WHERE entity_id = $1::uuid) AS "access_count!",
                (SELECT COUNT(*) FROM "DocumentBom" WHERE "documentId" = $1) AS "bom_count!",
                (SELECT COUNT(*) FROM "BomPart" WHERE "documentBomId" IN (SELECT id FROM "DocumentBom" WHERE "documentId" = $1)) AS "bom_part_count!",
                (SELECT "deletedAt"::timestamptz FROM "Document" WHERE id = $1) AS deleted_at
            "#, document_id, share_id).fetch_one(pool).await?;
        Ok(PurgeSnapshot {
            document_count: row.document_count,
            pin_count: row.pin_count,
            history_count: row.history_count,
            permission_count: row.permission_count,
            share_permission_count: row.share_permission_count,
            access_count: row.access_count,
            bom_count: row.bom_count,
            bom_part_count: row.bom_part_count,
            deleted_at: row.deleted_at,
        })
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("docx_example")))]
    async fn test_get_shas_for_deletion(pool: Pool<Postgres>) {
        let mut shas = get_shas_for_deletion(pool.clone(), "document-one")
            .await
            .unwrap();
        shas.sort();

        assert_eq!(
            shas,
            vec!["sha-1", "sha-1", "sha-2", "sha-2", "sha-3", "sha-4"]
        );
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn purge_removes_the_locked_document_and_its_captured_dependents(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        let fixture = purge_fixture(&pool).await?;
        let outcome = purge_deleted_document(&pool, PURGE_ID, fixture.token).await?;
        let DocumentPurgeOutcome::Purged(mut metadata) = outcome else {
            panic!("expected purge")
        };
        metadata.bom_shas.sort();
        assert_eq!(
            metadata,
            DocumentPurgeMetadata {
                document_id: PURGE_ID.into(),
                owner: "macro|user@user.com".into(),
                project_id: Some("purge-project".into()),
                file_type: Some("docx".into()),
                bom_shas: vec!["sha-a".into(), "sha-a".into(), "sha-b".into()],
            }
        );
        assert_eq!(
            purge_snapshot(&pool, PURGE_ID, &fixture.share_permission_id).await?,
            PurgeSnapshot {
                document_count: 0,
                pin_count: 0,
                history_count: 0,
                permission_count: 0,
                share_permission_count: 0,
                access_count: 0,
                bom_count: 0,
                bom_part_count: 0,
                deleted_at: None
            }
        );
        assert_eq!(
            purge_deleted_document(&pool, PURGE_ID, fixture.token).await?,
            DocumentPurgeOutcome::StaleOrUnavailable
        );
        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn purge_rollback_preserves_document_and_dependents(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        let fixture = purge_fixture(&pool).await?;
        sqlx::raw_sql(r#"CREATE FUNCTION fail_purge_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.id = '00000000-0000-0000-0000-000000000123' THEN RAISE EXCEPTION 'purge rollback sentinel'; END IF; RETURN OLD; END; $$"#).execute(&pool).await?;
        sqlx::raw_sql(r#"CREATE TRIGGER fail_purge_delete BEFORE DELETE ON "Document" FOR EACH ROW EXECUTE FUNCTION fail_purge_delete()"#).execute(&pool).await?;
        let error = purge_deleted_document(&pool, PURGE_ID, fixture.token)
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("purge rollback sentinel"));
        assert_eq!(
            purge_snapshot(&pool, PURGE_ID, &fixture.share_permission_id).await?,
            PurgeSnapshot {
                document_count: 1,
                pin_count: 1,
                history_count: 1,
                permission_count: 1,
                share_permission_count: 1,
                access_count: 1,
                bom_count: 1,
                bom_part_count: 3,
                deleted_at: Some(fixture.token)
            }
        );
        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn purge_serializes_with_restore_and_restore_wins(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        let fixture = purge_fixture(&pool).await?;
        let mut restore = pool.begin().await?;
        sqlx::query_scalar!(
            r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
            i64::from_be_bytes(*b"TASKDEPS")
        )
        .fetch_one(&mut *restore)
        .await?;
        sqlx::query!(
            r#"UPDATE "Document" SET "deletedAt" = NULL WHERE id = $1"#,
            PURGE_ID
        )
        .execute(&mut *restore)
        .await?;
        let purge_pool = pool.clone();
        let mut purge = tokio::spawn(async move {
            purge_deleted_document(&purge_pool, PURGE_ID, fixture.token).await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut purge)
                .await
                .is_err()
        );
        restore.commit().await?;
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), purge).await???,
            DocumentPurgeOutcome::StaleOrUnavailable
        );
        assert_eq!(
            purge_snapshot(&pool, PURGE_ID, &fixture.share_permission_id).await?,
            PurgeSnapshot {
                document_count: 1,
                pin_count: 1,
                history_count: 1,
                permission_count: 1,
                share_permission_count: 1,
                access_count: 1,
                bom_count: 1,
                bom_part_count: 3,
                deleted_at: None
            }
        );
        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn purge_rejects_live_missing_and_wrong_or_old_tokens(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        let old = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")?
            .with_timezone(&chrono::Utc);
        let fresh = chrono::DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?
            .with_timezone(&chrono::Utc);
        assert_eq!(
            purge_deleted_document(&pool, PURGE_ID, old).await?,
            DocumentPurgeOutcome::StaleOrUnavailable
        );
        insert_deleted_document(&pool, fresh).await?;
        sqlx::query!(
            r#"UPDATE "Document" SET "deletedAt" = NULL WHERE id = $1"#,
            PURGE_ID
        )
        .execute(&pool)
        .await?;
        assert_eq!(
            purge_deleted_document(&pool, PURGE_ID, old).await?,
            DocumentPurgeOutcome::StaleOrUnavailable
        );
        assert_eq!(
            sqlx::query!(
                r#"SELECT "deletedAt"::timestamptz AS deleted_at FROM "Document" WHERE id = $1"#,
                PURGE_ID
            )
            .fetch_one(&pool)
            .await?
            .deleted_at,
            None
        );
        sqlx::query!(
            r#"UPDATE "Document" SET "deletedAt" = $2 WHERE id = $1"#,
            PURGE_ID,
            fresh.naive_utc()
        )
        .execute(&pool)
        .await?;
        assert_eq!(
            purge_deleted_document(&pool, PURGE_ID, old).await?,
            DocumentPurgeOutcome::StaleOrUnavailable
        );
        let deleted_at = sqlx::query!(
            r#"SELECT "deletedAt"::timestamptz AS deleted_at FROM "Document" WHERE id = $1"#,
            PURGE_ID
        )
        .fetch_one(&pool)
        .await?
        .deleted_at;
        assert_eq!(deleted_at, Some(fresh));
        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn purge_invalid_uuid_has_safe_error(pool: Pool<Postgres>) -> anyhow::Result<()> {
        let token = chrono::Utc::now();
        let error = purge_deleted_document(&pool, "not-a-uuid", token)
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "invalid document id for purge");
        Ok(())
    }
}
