//! Typed REST handlers for project operational metadata.

use axum::{extract::State, Extension, Json};
use chrono::{DateTime, NaiveDate, Utc};
use entity_access::{
    domain::{
        models::{
            OwnerAccessLevel, ReadProjectWorkScoped, ViewAccessLevel, WriteProjectWorkStatusScoped,
        },
        ports::EntityAccessService,
    },
    inbound::axum_extractors::{MacroUserTeamExtractorV2, ProjectAccessLevelExtractor},
};
use macro_authorization::{MacroAuthorizationExtractor, MacroAuthorizationService, UserOnly};
use macro_user_id::user_id::MacroUserIdStr;
use model::response::TypedSuccessResponse;
use tower_http::request_id::RequestId;

use super::ProjectRouterState;
use crate::domain::{
    models::{
        ProjectError, ProjectOperationalStatus, ProjectOperations, ProjectPriority,
        ReplaceProjectOperationsArgs, UpdateProjectOperationsRequest,
    },
    ports::ProjectService,
};

/// Full replacement of client-owned operational project fields.
///
/// The project identity, acting user, team, request correlation, completion time,
/// and record timestamps are server-owned and therefore excluded.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplaceProjectOperationsRequest {
    /// Requested lifecycle state.
    pub status: ProjectOperationalStatus,
    /// Requested relative urgency.
    pub priority: ProjectPriority,
    /// Optional active-team lead. `null` or omission clears the lead.
    pub lead_user_id: Option<MacroUserIdStr<'static>>,
    /// Optional planned start date.
    pub start_date: Option<NaiveDate>,
    /// Optional planned target date.
    pub target_date: Option<NaiveDate>,
    /// Optional concise statement of the project's intended outcome.
    pub objective: Option<String>,
    /// Optional concise description of the next concrete action.
    pub next_action: Option<String>,
    /// Optional bounded object-shaped operational policy.
    #[schema(value_type = Option<Object>)]
    pub policy: Option<serde_json::Value>,
    /// Operational record version observed before this full replacement.
    pub expected_updated_at: DateTime<Utc>,
}

impl From<ReplaceProjectOperationsRequest> for ReplaceProjectOperationsArgs {
    fn from(request: ReplaceProjectOperationsRequest) -> Self {
        Self {
            status: request.status,
            priority: request.priority,
            lead_user_id: request.lead_user_id,
            start_date: request.start_date,
            target_date: request.target_date,
            objective: request.objective,
            next_action: request.next_action,
            policy: request.policy,
            expected_updated_at: request.expected_updated_at,
        }
    }
}

/// Successful read or replacement of a project's operational record.
pub type GetProjectOperationsResponse = TypedSuccessResponse<ProjectOperations>;

/// Read operational metadata for one project.
#[utoipa::path(
    tag = "project",
    get,
    operation_id = "get_project_operations",
    path = "/v2/projects/{id}/operations",
    params(("id" = String, Path, description = "ID of the project")),
    responses(
        (status = 200, body = inline(GetProjectOperationsResponse)),
        (status = 401, body = model::response::GenericErrorResponse),
        (status = 403, body = model::response::GenericErrorResponse),
        (status = 404, body = model::response::GenericErrorResponse),
        (status = 500, body = model::response::GenericErrorResponse),
    )
)]
#[tracing::instrument(skip(state, user, project_access, company_access), err)]
pub async fn get_project_operations_handler<T, Svc, Auth>(
    State(state): State<ProjectRouterState<T, Svc, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
    project_access: ProjectAccessLevelExtractor<ViewAccessLevel, Svc, Auth>,
    company_access: MacroUserTeamExtractorV2<ReadProjectWorkScoped, Svc, Auth>,
) -> Result<Json<GetProjectOperationsResponse>, ProjectError>
where
    T: ProjectService,
    Svc: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let operations = state
        .service
        .get_project_operations(
            project_access.entity_access_receipt,
            company_access.entity_access_receipt,
        )
        .await?;

    tracing::debug!(user_id = %user.authorization.macro_user_id, "read project operations");
    Ok(Json(GetProjectOperationsResponse {
        error: false,
        data: operations,
    }))
}

/// Replace all client-owned operational metadata for one project.
#[utoipa::path(
    tag = "project",
    put,
    operation_id = "replace_project_operations",
    path = "/v2/projects/{id}/operations",
    params(("id" = String, Path, description = "ID of the project")),
    request_body = ReplaceProjectOperationsRequest,
    responses(
        (status = 200, body = inline(GetProjectOperationsResponse)),
        (status = 400, body = model::response::GenericErrorResponse),
        (status = 401, body = model::response::GenericErrorResponse),
        (status = 403, body = model::response::GenericErrorResponse),
        (status = 404, body = model::response::GenericErrorResponse),
        (status = 409, body = model::response::GenericErrorResponse),
        (status = 500, body = model::response::GenericErrorResponse),
    )
)]
#[tracing::instrument(
    skip(state, user, project_access, company_access, request_id, body),
    err
)]
pub async fn replace_project_operations_handler<T, Svc, Auth>(
    State(state): State<ProjectRouterState<T, Svc, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
    project_access: ProjectAccessLevelExtractor<OwnerAccessLevel, Svc, Auth>,
    company_access: MacroUserTeamExtractorV2<WriteProjectWorkStatusScoped, Svc, Auth>,
    Extension(request_id): Extension<RequestId>,
    body: Result<Json<ReplaceProjectOperationsRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<GetProjectOperationsResponse>, ProjectError>
where
    T: ProjectService,
    Svc: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let Json(body) = body
        .map_err(|_| ProjectError::BadRequest("invalid project operations request".to_owned()))?;
    let request_id = request_id
        .header_value()
        .to_str()
        .map_err(|_| ProjectError::Internal(anyhow::anyhow!("invalid request identifier")))?
        .to_owned();
    let project_id = project_access
        .entity_access_receipt
        .entity()
        .entity_id
        .clone();
    let operations = state
        .service
        .update_project_operations(
            user.authorization.macro_user_id.clone(),
            project_access.entity_access_receipt,
            company_access.entity_access_receipt,
            UpdateProjectOperationsRequest {
                project_id,
                request_id,
                now: Utc::now(),
                replacement: body.into(),
            },
        )
        .await?;

    tracing::debug!(user_id = %user.authorization.macro_user_id, "replaced project operations");
    Ok(Json(GetProjectOperationsResponse {
        error: false,
        data: operations,
    }))
}
