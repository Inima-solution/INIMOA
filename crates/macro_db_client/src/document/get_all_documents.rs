use document_sub_type::DocumentSubType;
use macro_user_id::{cowlike::CowLike, user_id::MacroUserIdStr};
use model::document::DocumentMetadata;
use sqlx::{Pool, Postgres};

/// Used to get all documents in a paginated format
#[tracing::instrument(skip(db))]
pub async fn get_all_documents(
    db: &Pool<Postgres>,
    limit: i64,
    offset: i64,
) -> anyhow::Result<(Vec<DocumentMetadata>, i64)> {
    let count = sqlx::query!(
        r#"
        SELECT COUNT(*) as "count"
        FROM "Document" d
        WHERE d."deletedAt" IS NULL
        "#
    )
    .map(|row| row.count.unwrap_or(0))
    .fetch_one(db)
    .await?;

    if count == 0 {
        return Ok((vec![], 0));
    }

    let documents = sqlx::query!(
        r#"
        SELECT
            d.id as document_id,
            d.owner as owner,
            d.name as document_name,
            COALESCE(db.id, di.id) as "document_version_id!",
            d."branchedFromId" as "branched_from_id?",
            d."branchedFromVersionId" as "branched_from_version_id?",
            d."documentFamilyId" as "document_family_id?",
            d."fileType" as file_type,
            d."createdAt"::timestamptz as created_at,
            d."updatedAt"::timestamptz as updated_at,
            d."deletedAt"::timestamptz as deleted_at,
            db.bom_parts as "document_bom?",
            di.modification_data as "modification_data?",
            d."projectId" as "project_id?",
            p.name as "project_name?",
            di.sha as "sha?",
            dt.sub_type as "sub_type?: DocumentSubType"
        FROM
            "Document" d
        LEFT JOIN document_sub_type dt ON dt.document_id = d.id
        LEFT JOIN LATERAL (
            SELECT
                b.id,
                (
                    SELECT
                        json_agg(
                            json_build_object(
                                'id', bp.id,
                                'sha', bp.sha,
                                'path', bp.path
                            )
                        )
                    FROM
                        "BomPart" bp
                    WHERE
                        bp."documentBomId" = b.id
                ) as bom_parts
            FROM
                "DocumentBom" b
            WHERE
                b."documentId" = d.id
            ORDER BY
                b."createdAt" DESC
            LIMIT 1
        ) db ON d."fileType" = 'docx'
        LEFT JOIN LATERAL (
            SELECT
                i.id,
                i."documentId",
                i."sha",
                i."createdAt",
                (
                    SELECT
                        imod."modificationData"
                    FROM
                        "DocumentInstanceModificationData" imod
                    WHERE
                        imod."documentInstanceId" = i.id
                ) as modification_data,
                i."updatedAt"
            FROM
                "DocumentInstance" i
            WHERE
                i."documentId" = d.id
            ORDER BY
                i."updatedAt" DESC
            LIMIT 1
        ) di ON d."fileType" IS DISTINCT FROM 'docx'
        LEFT JOIN "Project" p ON p.id = d."projectId"
        WHERE
        d."deletedAt" IS NULL
        ORDER BY d."createdAt" DESC
        LIMIT $1 OFFSET $2

    "#,
        limit,
        offset
    )
    .try_map(|row| {
        Ok(DocumentMetadata {
            document_id: row.document_id,
            document_version_id: row.document_version_id,
            owner: MacroUserIdStr::parse_from_str(&row.owner)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?
                .into_owned(),
            document_name: row.document_name,
            file_type: row.file_type,
            sha: row.sha,
            project_id: row.project_id,
            project_name: row.project_name,
            branched_from_id: row.branched_from_id,
            branched_from_version_id: row.branched_from_version_id,
            document_family_id: row.document_family_id,
            document_bom: row.document_bom,
            modification_data: row.modification_data,
            created_at: row.created_at,
            updated_at: row.updated_at,
            sub_type: row.sub_type,
            deleted_at: row.deleted_at,
        })
    })
    .fetch_all(db)
    .await?;

    Ok((documents, count))
}

#[tracing::instrument(skip(db))]
pub async fn get_documents_to_delete(
    db: &Pool<Postgres>,
    date: &chrono::NaiveDateTime,
) -> anyhow::Result<Vec<DocumentPurgeCandidate>> {
    let result = sqlx::query!(
        r#"
            SELECT d.id, d."deletedAt"::timestamptz AS "deleted_at!"
            FROM "Document" d
            WHERE d."deletedAt" IS NOT NULL AND d."deletedAt" <= $1
        "#,
        date
    )
    .map(|row| DocumentPurgeCandidate {
        document_id: row.id,
        deleted_at: row.deleted_at,
    })
    .fetch_all(db)
    .await?;

    Ok(result)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPurgeCandidate {
    pub document_id: String,
    pub deleted_at: chrono::DateTime<chrono::Utc>,
}

/// Returns a paginated list of document IDs, sorting by ascending so we don't miss new ones
#[tracing::instrument(skip(db))]
pub async fn get_all_document_ids_paginated(
    db: &sqlx::Pool<sqlx::Postgres>,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<String>> {
    let result = sqlx::query!(
        r#"
        SELECT
            id as "document_id"
        FROM
            "Document"
        WHERE
            "deletedAt" IS NULL
        ORDER BY
            "createdAt" ASC
        LIMIT $1
        OFFSET $2
        "#,
        limit,
        offset
    )
    .map(|row| row.document_id)
    .fetch_all(db)
    .await?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_lots_of_documents")))]
    async fn test_get_all_documents(pool: Pool<Postgres>) -> anyhow::Result<()> {
        let documents = get_all_documents(&pool, 3, 0).await?;

        assert_eq!(documents.1, 7);
        assert_eq!(documents.0.len(), 3);

        Ok(())
    }

    #[sqlx::test(fixtures(path = "../../fixtures", scripts("basic_user_with_document")))]
    async fn delete_candidates_preserve_exact_timestamp_and_cutoff(
        pool: Pool<Postgres>,
    ) -> anyhow::Result<()> {
        let selected = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00.123Z")?
            .with_timezone(&chrono::Utc);
        let newer = chrono::DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z")?
            .with_timezone(&chrono::Utc);
        sqlx::query!(
            r#"
            INSERT INTO "Document" (id, name, owner, "deletedAt")
            VALUES
                ('00000000-0000-0000-0000-000000000201', 'selected', 'macro|user@user.com', $1),
                ('00000000-0000-0000-0000-000000000202', 'newer', 'macro|user@user.com', $2),
                ('00000000-0000-0000-0000-000000000203', 'live', 'macro|user@user.com', NULL)
            "#,
            selected.naive_utc(),
            newer.naive_utc()
        )
        .execute(&pool)
        .await?;
        let cutoff = chrono::DateTime::parse_from_rfc3339("2026-01-15T00:00:00Z")?.naive_utc();
        let mut candidates = get_documents_to_delete(&pool, &cutoff).await?;
        candidates.sort_by(|left, right| left.document_id.cmp(&right.document_id));
        assert_eq!(
            candidates,
            vec![DocumentPurgeCandidate {
                document_id: "00000000-0000-0000-0000-000000000201".into(),
                deleted_at: selected
            }]
        );
        Ok(())
    }
}
