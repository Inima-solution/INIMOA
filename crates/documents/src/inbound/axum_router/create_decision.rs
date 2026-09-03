//! Handler for `POST /documents/create_decision`.

use axum::{Json, extract::State};
use entity_access::domain::ports::EntityAccessService;
use entity_access::inbound::axum_extractors::ProjectBodyAccessLevelExtractorV2;
use macro_authorization::{MacroAuthorizationExtractor, MacroAuthorizationService, UserOrInternal};
use models_permissions::share_permission::access_level::EditAccessLevel;

use super::DocumentRouterState;
use crate::domain::create::{MarkdownSubtype, NewDocumentMetadata, NewMarkdownTextDocument};
use crate::domain::models::{CreateDecisionRequest, CreateDecisionResponse, DocumentError};
use crate::domain::ports::DocumentService;
use crate::domain::ports::create::DocumentCreationService;

/// Create a project-scoped Decision backed by collaborative markdown.
#[utoipa::path(
    tag = "document",
    post,
    path = "/documents/create_decision",
    request_body = CreateDecisionRequest,
    responses(
        (status = 200, body = inline(CreateDecisionResponse)),
        (status = 400, body = model_error_response::ErrorResponse),
        (status = 401, body = model_error_response::ErrorResponse),
        (status = 500, body = model_error_response::ErrorResponse),
    )
)]
#[tracing::instrument(skip(state, user, project), fields(user_id=?user.authorization.user.macro_user_id))]
pub async fn create_decision_handler<
    T: DocumentService + DocumentCreationService,
    Svc: EntityAccessService,
    Auth: MacroAuthorizationService,
>(
    State(state): State<DocumentRouterState<T, Svc, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    project: ProjectBodyAccessLevelExtractorV2<EditAccessLevel, CreateDecisionRequest, Svc, Auth>,
) -> Result<Json<CreateDecisionResponse>, DocumentError> {
    let req = project.into_inner();
    let created = state
        .creator
        .create_markdown_text(
            user.authorization.user.macro_user_id.clone(),
            NewMarkdownTextDocument {
                metadata: NewDocumentMetadata::builder(req.decision_name)
                    .project_id(req.project_id)
                    .build(),
                markdown: req.markdown.unwrap_or_default(),
                subtype: MarkdownSubtype::Decision,
            },
        )
        .await?;

    Ok(Json(CreateDecisionResponse {
        document_id: created.document_id().to_string(),
    }))
}
