//! Project-scoped REST adapter for the computed task-dependency readiness read model.

use axum::{
    Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use entity_access::domain::{
    models::{AccessError, EntityType, ReadProjectWorkScoped, ViewAccessLevel},
    ports::EntityAccessService,
};
use macro_authorization::{MacroAuthorizationExtractor, MacroAuthorizationService, UserOnly};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

use super::PropertiesRouterState;
use crate::domain::{
    error::PropertiesErr,
    model::{TaskDependencyReadiness, ViewReceipt},
    service::{ProjectWorkReadReceipt, PropertiesService},
};

/// Maximum number of source task IDs accepted by the public batch request.
const TASK_DEPENDENCY_READINESS_BATCH_MAX: usize = 200;

/// Request body for the bounded task-dependency readiness read.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskDependencyReadinessBatchRequest {
    /// Source task IDs to evaluate. An empty valid request returns `[]`.
    #[schema(max_items = 200)]
    pub task_ids: Vec<Uuid>,
}

/// Successful response is deliberately a raw array, not a generic envelope.
pub type GetProjectTaskDependencyReadinessResponse = Vec<TaskDependencyReadiness>;

/// Stable, redacted error envelope for task-dependency readiness requests.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskDependencyReadinessErrorResponse {
    pub error: bool,
    pub message: String,
}

/// Fixed, client-safe failures for the task-dependency readiness endpoint.
#[derive(Debug, Error)]
pub enum ProjectTaskDependencyReadinessError {
    #[error("invalid request")]
    BadRequest,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("internal server error")]
    Internal,
}

impl ProjectTaskDependencyReadinessError {
    fn authorization(status: StatusCode) -> Self {
        match status {
            StatusCode::BAD_REQUEST => Self::BadRequest,
            StatusCode::UNAUTHORIZED => Self::Unauthorized,
            StatusCode::FORBIDDEN => Self::Forbidden,
            _ => Self::Internal,
        }
    }

    fn from_access(error: AccessError) -> Self {
        match error {
            AccessError::BadRequest(_) => Self::BadRequest,
            // The project existence middleware has already distinguished a
            // missing/deleted project. Receipt failures here are authorization
            // denials and must not reveal project facts.
            AccessError::Unauthorized
            | AccessError::UnauthorizedWithMessage(_)
            | AccessError::NotFound(_) => Self::Forbidden,
            AccessError::Unavailable(_) | AccessError::Internal(_) => Self::Internal,
        }
    }

    fn from_properties(error: PropertiesErr) -> Self {
        match error {
            PropertiesErr::Validation(_) => Self::BadRequest,
            PropertiesErr::NotFound | PropertiesErr::TaskDependenciesUnavailable => Self::NotFound,
            PropertiesErr::PermissionDenied
            | PropertiesErr::SystemPropertyNotModifiable
            | PropertiesErr::RequiredProperty
            | PropertiesErr::TeamMembershipRequired => Self::Forbidden,
            PropertiesErr::OptionNotFound
            | PropertiesErr::DuplicateOptionValue
            | PropertiesErr::ConflictingTeamLabel(_)
            | PropertiesErr::TaskDependencyCycle
            | PropertiesErr::Repo(_)
            | PropertiesErr::PermissionServiceNotConfigured
            | PropertiesErr::EntityPropertyNotFound => Self::Internal,
        }
    }
}

impl IntoResponse for ProjectTaskDependencyReadinessError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest => (StatusCode::BAD_REQUEST, "invalid request".to_owned()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_owned()),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden".to_owned()),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found".to_owned()),
            // Do not expose repository errors or inaccessible dependency facts.
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error".to_owned(),
            ),
        };
        if status.is_server_error() {
            tracing::error!("task dependency readiness request failed");
        }
        (
            status,
            Json(TaskDependencyReadinessErrorResponse {
                error: true,
                message,
            }),
        )
            .into_response()
    }
}

/// Build only the properties-owned project readiness route. The DSS composition
/// root applies the existing project-existence middleware before nesting it.
pub fn project_dependency_readiness_router<S, A, Auth>() -> Router<PropertiesRouterState<S, A, Auth>>
where
    S: PropertiesService,
    A: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    Router::new().route(
        "/{id}/task-dependency-readiness",
        post(get_project_task_dependency_readiness::<S, A, Auth>),
    )
}

/// Return direct dependency readiness for project-scoped source tasks.
#[utoipa::path(
    tag = "project",
    post,
    operation_id = "getProjectTaskDependencyReadiness",
    path = "/v2/projects/{id}/task-dependency-readiness",
    params(("id" = String, Path, description = "ID of the project")),
    request_body = TaskDependencyReadinessBatchRequest,
    responses(
        (status = 200, body = inline(GetProjectTaskDependencyReadinessResponse)),
        (status = 400, body = TaskDependencyReadinessErrorResponse),
        (status = 401, body = TaskDependencyReadinessErrorResponse),
        (status = 403, body = TaskDependencyReadinessErrorResponse),
        (status = 404, body = TaskDependencyReadinessErrorResponse),
        (status = 500, body = TaskDependencyReadinessErrorResponse),
    )
)]
#[tracing::instrument(skip(state, user, body), err)]
pub async fn get_project_task_dependency_readiness<S, A, Auth>(
    user: Result<
        MacroAuthorizationExtractor<Auth, UserOnly>,
        macro_authorization::MacroAuthorizationRejection,
    >,
    Path(id): Path<String>,
    State(state): State<PropertiesRouterState<S, A, Auth>>,
    body: Result<Json<TaskDependencyReadinessBatchRequest>, JsonRejection>,
) -> Result<Json<GetProjectTaskDependencyReadinessResponse>, ProjectTaskDependencyReadinessError>
where
    S: PropertiesService,
    A: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    // Keep the authorization sequence explicit: human UserOnly, Project View,
    // same-principal ReadProjectWorkScoped, then the Phase-A service call.
    let user =
        user.map_err(|error| ProjectTaskDependencyReadinessError::authorization(error.status))?;
    let project: ViewReceipt = state
        .entity_access_service
        .generate_entity_access_receipt::<ViewAccessLevel>(
            &user.authorization.macro_user_id,
            None,
            &id,
            EntityType::Project,
        )
        .await
        .map_err(ProjectTaskDependencyReadinessError::from_access)?;

    let team_info = state
        .entity_access_service
        .get_user_team(&user.authorization.macro_user_id)
        .await
        .map_err(ProjectTaskDependencyReadinessError::from_access)?
        .ok_or(ProjectTaskDependencyReadinessError::Forbidden)?;
    let team: ProjectWorkReadReceipt = state
        .entity_access_service
        .generate_entity_access_receipt::<ReadProjectWorkScoped>(
            &user.authorization.macro_user_id,
            None,
            &team_info.team_id.to_string(),
            EntityType::Team,
        )
        .await
        .map_err(ProjectTaskDependencyReadinessError::from_access)?;

    let Json(body) = body.map_err(|_| ProjectTaskDependencyReadinessError::BadRequest)?;
    if body.task_ids.len() > TASK_DEPENDENCY_READINESS_BATCH_MAX {
        return Err(ProjectTaskDependencyReadinessError::BadRequest);
    }

    let readiness = state
        .properties_service
        .get_task_dependency_readiness(&project, &team, &body.task_ids)
        .await
        .map_err(ProjectTaskDependencyReadinessError::from_properties)?;
    Ok(Json(readiness))
}
