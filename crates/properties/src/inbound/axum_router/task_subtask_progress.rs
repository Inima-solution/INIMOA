//! Generic properties REST adapter for computed direct-subtask progress.

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
    model::{TaskSubtaskProgress, ViewReceipt},
    service::PropertiesService,
};

const TASK_SUBTASK_PROGRESS_BATCH_MAX: usize = 200;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskSubtaskProgressBatchRequest {
    #[schema(max_items = 200)]
    pub task_ids: Vec<Uuid>,
}

pub type TaskSubtaskProgressResponse = Vec<TaskSubtaskProgress>;

#[derive(Debug, Serialize, ToSchema)]
pub struct TaskSubtaskProgressErrorResponse {
    pub error: bool,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum TaskSubtaskProgressError {
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

impl TaskSubtaskProgressError {
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

impl IntoResponse for TaskSubtaskProgressError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest => (StatusCode::BAD_REQUEST, "invalid request"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found"),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal server error"),
        };
        if status.is_server_error() {
            tracing::error!("task subtask progress request failed");
        }
        (
            status,
            Json(TaskSubtaskProgressErrorResponse {
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
    operation_id = "getTaskSubtaskProgress",
    path = "/properties/task-subtask-progress",
    request_body = TaskSubtaskProgressBatchRequest,
    responses(
        (status = 200, body = inline(TaskSubtaskProgressResponse)),
        (status = 400, body = TaskSubtaskProgressErrorResponse),
        (status = 401, body = TaskSubtaskProgressErrorResponse),
        (status = 403, body = TaskSubtaskProgressErrorResponse),
        (status = 404, body = TaskSubtaskProgressErrorResponse),
        (status = 500, body = TaskSubtaskProgressErrorResponse),
    )
)]
#[tracing::instrument(skip(state, user, body), err)]
pub async fn get_task_subtask_progress<S, A, Auth>(
    user: Result<
        MacroAuthorizationExtractor<Auth, UserOnly>,
        macro_authorization::MacroAuthorizationRejection,
    >,
    State(state): State<PropertiesRouterState<S, A, Auth>>,
    body: Result<Json<TaskSubtaskProgressBatchRequest>, JsonRejection>,
) -> Result<Json<TaskSubtaskProgressResponse>, TaskSubtaskProgressError>
where
    S: PropertiesService,
    A: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let user = user.map_err(|error| TaskSubtaskProgressError::authorization(error.status))?;
    let Json(body) = body.map_err(|_| TaskSubtaskProgressError::BadRequest)?;
    if body.task_ids.len() > TASK_SUBTASK_PROGRESS_BATCH_MAX {
        return Err(TaskSubtaskProgressError::BadRequest);
    }
    let mut sources: Vec<ViewReceipt> = Vec::with_capacity(body.task_ids.len());
    for task_id in &body.task_ids {
        let source = state
            .entity_access_service
            .generate_entity_access_receipt::<ViewAccessLevel>(
                &user.authorization.macro_user_id,
                None,
                &task_id.to_string(),
                EntityType::Document,
            )
            .await
            .map_err(TaskSubtaskProgressError::access)?;
        sources.push(source);
    }
    let progress = state
        .properties_service
        .get_task_subtask_progress(&sources, &body.task_ids)
        .await
        .map_err(TaskSubtaskProgressError::properties)?;
    Ok(Json(progress))
}
