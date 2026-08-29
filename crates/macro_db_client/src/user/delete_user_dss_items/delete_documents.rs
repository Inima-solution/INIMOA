/// Classifies every document owned by a user in deterministic order.
///
/// The caller classifies this complete set before deleting any document rows so
/// transaction-local lifecycle cleanup can still inspect task sources.
#[tracing::instrument(skip(transaction))]
pub async fn classify_user_documents(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
) -> anyhow::Result<Vec<String>> {
    let mut document_ids = sqlx::query!(
        r#"
        SELECT id FROM "Document" WHERE "owner" = $1
    "#,
        user_id
    )
    .map(|row| row.id)
    .fetch_all(transaction.as_mut())
    .await?;
    document_ids.sort();
    Ok(document_ids)
}

/// Deletes the already-classified documents for a user.
/// Does not commit the transaction.
#[tracing::instrument(skip(transaction, user_documents))]
pub async fn delete_user_documents(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_documents: &[String],
) -> anyhow::Result<()> {
    // Delete pins
    sqlx::query!(
        r#"
        DELETE FROM "Pin" 
        WHERE "pinnedItemId" = ANY($1) AND "pinnedItemType" = $2
        "#,
        &user_documents,
        "document"
    )
    .execute(transaction.as_mut())
    .await?;

    // Delete user history
    sqlx::query!(
        r#"
        DELETE FROM "UserHistory" 
        WHERE "itemId" = ANY($1) AND "itemType" = $2
        "#,
        &user_documents,
        "document"
    )
    .execute(transaction.as_mut())
    .await?;

    // Delete permissions
    sqlx::query!(
        r#"
        DELETE FROM "SharePermission" sp
        USING "DocumentPermission" dp 
        WHERE dp."sharePermissionId" = sp.id
        AND dp."documentId" = ANY($1)
    "#,
        &user_documents
    )
    .execute(transaction.as_mut())
    .await?;

    // Delete chats
    sqlx::query!(
        r#"
        DELETE FROM "Document" 
        WHERE id = ANY($1)
        "#,
        &user_documents
    )
    .execute(transaction.as_mut())
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{Pool, Postgres};
    use uuid::Uuid;

    #[sqlx::test(fixtures(
        path = "../../../fixtures",
        scripts("basic_user_with_lots_of_documents")
    ))]
    async fn test_delete_user_documents(pool: Pool<Postgres>) -> anyhow::Result<()> {
        let other_macro_user_id = Uuid::parse_str("b1111111-1111-1111-1111-111111111111")?;
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
        .execute(&pool)
        .await?;
        sqlx::query(r#"INSERT INTO "User" (id, email, macro_user_id) VALUES ($1, $2, $3)"#)
            .bind("macro|other@user.com")
            .bind("other@user.com")
            .bind(other_macro_user_id)
            .execute(&pool)
            .await?;
        sqlx::query(
            r#"
            INSERT INTO "Document" (id, name, "fileType", owner, "createdAt", "updatedAt")
            VALUES ($1, $2, $3, $4, NOW(), NOW())
            "#,
        )
        .bind("document-other-owner")
        .bind("other owner's document")
        .bind("txt")
        .bind("macro|other@user.com")
        .execute(&pool)
        .await?;
        let mut transaction = pool.begin().await?;
        let result = classify_user_documents(&mut transaction, "macro|user@user.com").await?;
        delete_user_documents(&mut transaction, &result).await?;
        transaction.commit().await?;

        assert_eq!(
            result,
            vec![
                "document-deleted".to_string(),
                "document-five".to_string(),
                "document-four".to_string(),
                "document-one".to_string(),
                "document-seven".to_string(),
                "document-six".to_string(),
                "document-three".to_string(),
                "document-two".to_string()
            ]
        );

        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Document" WHERE "owner" = $1"#)
                .bind("macro|user@user.com")
                .fetch_one(&pool)
                .await?,
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Document" WHERE id = $1"#)
                .bind("document-other-owner")
                .fetch_one(&pool)
                .await?,
            1
        );

        Ok(())
    }
}
