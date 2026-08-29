use crate::api::context::ApiContext;
use crate::api::context::{AuthorizationService, EntityAccessService};
use crate::api::util::count_occurrences;
use crate::service::document_event_publisher::publish_document_purged_event;
use axum::Json;
use axum::extract::State;
use axum::response::Response;
use axum::{Extension, extract::Path, http::StatusCode, response::IntoResponse};
use entity_access::inbound::axum_extractors::DocumentAccessExtractor;
#[allow(unused_imports)]
use futures::stream::TryStreamExt;
use macro_authorization::{MacroAuthorizationExtractor, UserOrInternal};
use model::document::DocumentBasic;
use model::response::{
    ErrorResponse, GenericErrorResponse, GenericResponse, GenericSuccessResponse, SuccessResponse,
};
use models_permissions::share_permission::access_level::OwnerAccessLevel;
use serde::Deserialize;

#[cfg(test)]
mod test;

#[derive(Deserialize)]
pub struct Params {
    pub document_id: String,
}

fn require_purged(
    outcome: macro_db_client::document::DocumentPurgeOutcome,
) -> Option<macro_db_client::document::DocumentPurgeMetadata> {
    match outcome {
        macro_db_client::document::DocumentPurgeOutcome::Purged(metadata) => Some(metadata),
        macro_db_client::document::DocumentPurgeOutcome::StaleOrUnavailable => None,
    }
}

/// Permanently deletes a document.
#[utoipa::path(
        tag = "document",
        delete,
        operation_id = "permanently_delete_document",
        path = "/documents/{document_id}/permanent",
        params(
            ("document_id" = String, Path, description = "Document ID")
        ),
        responses(
            (status = 200, body=SuccessResponse),
            (status = 401, body=GenericErrorResponse),
            (status = 404, body=GenericErrorResponse),
            (status = 500, body=GenericErrorResponse),
        )
    )]
#[tracing::instrument(skip(state, user, _access), fields(user_id=?user.authorization.user.macro_user_id))]
pub async fn permanently_delete_document_handler(
    _access: DocumentAccessExtractor<OwnerAccessLevel, EntityAccessService, AuthorizationService>,
    State(state): State<ApiContext>,
    user: MacroAuthorizationExtractor<AuthorizationService, UserOrInternal>,
    document_context: Extension<DocumentBasic>,
    Path(Params { document_id }): Path<Params>,
) -> Result<Response, Response> {
    tracing::info!("permanently_delete_document");

    let deleted_at = document_context.deleted_at.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                message: "document not found".into(),
            }),
        )
            .into_response()
    })?;
    let metadata =
        macro_db_client::document::purge_deleted_document(&state.db, &document_id, deleted_at)
            .await
            .map_err(|e| {
                tracing::error!(error=?e, "unable to purge document");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        message: "unable to delete document".into(),
                    }),
                )
                    .into_response()
            })?;
    let metadata = match require_purged(metadata) {
        Some(metadata) => metadata,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    message: "document not found".into(),
                }),
            )
                .into_response());
        }
    };
    if metadata.file_type.as_deref() == Some("docx") {
        state
            .redis_client
            .decrement_counts(&count_occurrences(metadata.bom_shas))
            .await
            .map_err(|e| {
                tracing::error!(error=?e, "unable to decrement sha ref counts");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        message: "unable to decrement sha ref counts".into(),
                    }),
                )
                    .into_response()
            })?;
    }

    // Delete entity mentions where this doc is the source
    if let Err(e) = comms_db_client::entity_mentions::delete_entity_mentions_by_source(
        &state.db,
        vec![document_id.clone()],
    )
    .await
    {
        tracing::error!(error=?e, "unable to delete entity mentions");
    }

    // Queue document for deletion
    state
        .sqs_client
        .enqueue_document_delete(&metadata.owner, &document_id)
        .await
        .map_err(|e| {
            tracing::error!(error=?e, "unable to enqueue document delete");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    message: "unable to enqueue document delete".into(),
                }),
            )
                .into_response()
        })?;

    publish_document_purged_event(&state.macro_event_broker, &document_id).map_err(|e| {
        tracing::error!(error=?e, "unable to publish document purged event");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                message: "unable to publish document purged event".into(),
            }),
        )
            .into_response()
    })?;

    let response_data = GenericSuccessResponse { success: true };

    Ok(GenericResponse::builder()
        .data(&response_data)
        .send(StatusCode::OK))
}
