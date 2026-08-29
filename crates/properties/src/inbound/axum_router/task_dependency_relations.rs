//! REST adapter for caller-scoped task dependency relations.

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use entity_access::domain::{
    models::{AccessError, EntityType, ViewAccessLevel},
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
    model::{TaskDependencyRelations, ViewReceipt},
    service::PropertiesService,
};

const TASK_DEPENDENCY_RELATIONS_BATCH_MAX: usize = 200;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskDependencyRelationsBatchRequest {
    #[schema(max_items = 200)]
    pub task_ids: Vec<Uuid>,
}
pub type TaskDependencyRelationsResponse = Vec<TaskDependencyRelations>;
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskDependencyRelationsErrorResponse {
    pub error: bool,
    pub message: String,
}
#[derive(Debug, Error)]
pub enum TaskDependencyRelationsError {
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
impl TaskDependencyRelationsError {
    fn authorization(status: StatusCode) -> Self {
        match status {
            StatusCode::BAD_REQUEST => Self::BadRequest,
            StatusCode::UNAUTHORIZED => Self::Unauthorized,
            StatusCode::FORBIDDEN => Self::Forbidden,
            _ => Self::Internal,
        }
    }
    fn access(error: AccessError) -> Self {
        match error {
            AccessError::BadRequest(_) => Self::BadRequest,
            AccessError::Unauthorized | AccessError::UnauthorizedWithMessage(_) => Self::Forbidden,
            AccessError::NotFound(_) => Self::NotFound,
            AccessError::Unavailable(_) | AccessError::Internal(_) => Self::Internal,
        }
    }
    fn properties(error: PropertiesErr) -> Self {
        match error {
            PropertiesErr::Validation(_) => Self::BadRequest,
            PropertiesErr::NotFound | PropertiesErr::TaskDependenciesUnavailable => Self::NotFound,
            PropertiesErr::PermissionDenied
            | PropertiesErr::SystemPropertyNotModifiable
            | PropertiesErr::RequiredProperty
            | PropertiesErr::TeamMembershipRequired => Self::Forbidden,
            _ => Self::Internal,
        }
    }
}
impl IntoResponse for TaskDependencyRelationsError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest => (StatusCode::BAD_REQUEST, "invalid request"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found"),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal server error"),
        };
        if status.is_server_error() {
            tracing::error!("task dependency relations request failed");
        }
        (
            status,
            Json(TaskDependencyRelationsErrorResponse {
                error: true,
                message: message.to_owned(),
            }),
        )
            .into_response()
    }
}

#[utoipa::path(
    tag = "properties",
    post,
    operation_id = "getTaskDependencyRelations",
    path = "/properties/task-dependency-relations",
    request_body = TaskDependencyRelationsBatchRequest,
    responses(
        (status = 200, body = inline(TaskDependencyRelationsResponse)),
        (status = 400, body = TaskDependencyRelationsErrorResponse),
        (status = 401, body = TaskDependencyRelationsErrorResponse),
        (status = 403, body = TaskDependencyRelationsErrorResponse),
        (status = 404, body = TaskDependencyRelationsErrorResponse),
        (status = 500, body = TaskDependencyRelationsErrorResponse),
    )
)]
#[tracing::instrument(skip(state, user, body), err)]
pub async fn get_task_dependency_relations<S, A, Auth>(
    user: Result<
        MacroAuthorizationExtractor<Auth, UserOnly>,
        macro_authorization::MacroAuthorizationRejection,
    >,
    State(state): State<PropertiesRouterState<S, A, Auth>>,
    body: Result<Json<TaskDependencyRelationsBatchRequest>, JsonRejection>,
) -> Result<Json<TaskDependencyRelationsResponse>, TaskDependencyRelationsError>
where
    S: PropertiesService,
    A: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let user = user.map_err(|error| TaskDependencyRelationsError::authorization(error.status))?;
    let Json(body) = body.map_err(|_| TaskDependencyRelationsError::BadRequest)?;
    if body.task_ids.len() > TASK_DEPENDENCY_RELATIONS_BATCH_MAX {
        return Err(TaskDependencyRelationsError::BadRequest);
    }
    let mut sources: Vec<ViewReceipt> = Vec::with_capacity(body.task_ids.len());
    for task_id in &body.task_ids {
        sources.push(
            state
                .entity_access_service
                .generate_entity_access_receipt::<ViewAccessLevel>(
                    &user.authorization.macro_user_id,
                    None,
                    &task_id.to_string(),
                    EntityType::Document,
                )
                .await
                .map_err(TaskDependencyRelationsError::access)?,
        );
    }
    Ok(Json(
        state
            .properties_service
            .get_task_dependency_relations(&sources, &body.task_ids)
            .await
            .map_err(TaskDependencyRelationsError::properties)?,
    ))
}
