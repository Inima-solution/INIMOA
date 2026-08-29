use crate::api::context::{ApiContext, AuthorizationService};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use macro_authorization::{InternalOnly, MacroAuthorizationExtractor};
use model::response::{EmptyResponse, GenericErrorResponse};
use sqlx::{Postgres, Transaction};

#[derive(serde::Deserialize)]
pub struct Params {
    pub user_id: String,
}

/// deletes the users items
#[utoipa::path(
        delete,
        path = "/users/{user_id}/items",
        operation_id = "delete_user_items",
        params(
            ("user_id" = String, Path, description = "ID of the user")
        ),
        responses(
            (status = 200, body=EmptyResponse),
            (status = 401, body=GenericErrorResponse),
            (status = 500, body=GenericErrorResponse),
        )
    )]
#[tracing::instrument(skip(ctx, _auth))]
pub async fn delete_user_items_handler(
    State(ctx): State<ApiContext>,
    _auth: MacroAuthorizationExtractor<AuthorizationService, InternalOnly>,
    Path(Params { user_id }): Path<Params>,
) -> Result<Response, Response> {
    tracing::info!("deleting user dss items");
    let mut transaction = ctx.db.begin().await.map_err(|e| {
        tracing::error!(error=?e, "failed to begin transaction");
        (StatusCode::INTERNAL_SERVER_ERROR).into_response()
    })?;

    let document_ids = delete_user_items_in_transaction(&mut transaction, &user_id)
        .await
        .map_err(|e| {
            tracing::error!(error=?e, "failed to delete user items");
            (StatusCode::INTERNAL_SERVER_ERROR).into_response()
        })?;

    transaction.commit().await.map_err(|e| {
        tracing::error!(error=?e, "failed to commit transaction");
        (StatusCode::INTERNAL_SERVER_ERROR).into_response()
    })?;

    let document_ids_with_owner = document_cleanup_queue(&document_ids, &user_id);

    if let Err(e) = ctx
        .sqs_client
        .bulk_enqueue_document_delete_with_owner(document_ids_with_owner)
        .await
    {
        tracing::error!(error=?e, "failed to enqueue document delete");
    }

    Ok((StatusCode::OK).into_response())
}

/// Runs all durable user-item deletion work. External cleanup is deliberately
/// excluded: callers enqueue only after this transaction commits.
async fn delete_user_items_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> anyhow::Result<Vec<String>> {
    // Keep the shared task lifecycle lock order. Both are xact locks, so they
    // are acquired once for this owner purge and released at commit/rollback.
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(transaction.as_mut())
    .await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKHIER")
    )
    .fetch_one(transaction.as_mut())
    .await?;

    let document_ids =
        macro_db_client::user::delete_user_dss_items::delete_documents::classify_user_documents(
            transaction,
            user_id,
        )
        .await?;

    system_properties::outbound::task_hierarchy_lifecycle::purge_confirmed_task_hierarchy(
        transaction.as_mut(),
        &document_ids,
    )
    .await?;

    macro_db_client::user::delete_user_dss_items::delete_documents::delete_user_documents(
        transaction,
        &document_ids,
    )
    .await?;
    macro_db_client::user::delete_user_dss_items::delete_chats::delete_user_chats(
        transaction,
        user_id,
    )
    .await?;
    macro_db_client::user::delete_user_dss_items::delete_projects::delete_user_projects(
        transaction,
        user_id,
    )
    .await?;

    Ok(document_ids)
}

fn document_cleanup_queue(document_ids: &[String], user_id: &str) -> Vec<(String, String)> {
    document_ids
        .iter()
        .cloned()
        .map(|id| (id, user_id.to_owned()))
        .collect()
}

#[cfg(test)]
mod test;
