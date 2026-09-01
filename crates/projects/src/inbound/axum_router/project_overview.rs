//! Typed REST handler for the bounded project overview.

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};
use chrono::NaiveDate;
use entity_access::{
    domain::{
        models::{ReadProjectWorkScoped, ViewAccessLevel},
        ports::EntityAccessService,
    },
    inbound::axum_extractors::{MacroUserTeamExtractorV2, ProjectAccessLevelExtractor},
};
use macro_authorization::{MacroAuthorizationExtractor, MacroAuthorizationService, UserOnly};
use model::response::TypedSuccessResponse;
use serde::Deserialize;

use super::ProjectRouterState;
use crate::domain::{
    models::{ProjectError, ProjectOverview},
    ports::ProjectService,
};

/// Successful bounded project-overview response.
pub type GetProjectOverviewResponse = TypedSuccessResponse<ProjectOverview>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Required overview calendar-date query parameters.
pub struct GetProjectOverviewQuery {
    /// Caller-selected calendar date in YYYY-MM-DD form.
    as_of_date: String,
}

/// Read the canonical, bounded overview for one project.
#[utoipa::path(
    tag = "project",
    get,
    operation_id = "get_project_overview",
    path = "/v2/projects/{id}/overview",
    params(
        ("id" = String, Path, description = "ID of the project"),
        ("asOfDate" = String, Query, description = "Calendar date in YYYY-MM-DD format")
    ),
    responses(
        (status = 200, body = inline(GetProjectOverviewResponse)),
        (status = 400, body = model::response::GenericErrorResponse),
        (status = 401, body = model::response::GenericErrorResponse),
        (status = 403, body = model::response::GenericErrorResponse),
        (status = 404, body = model::response::GenericErrorResponse),
        (status = 500, body = model::response::GenericErrorResponse),
    )
)]
#[tracing::instrument(skip(state, user, project_access, company_access), err)]
pub async fn get_project_overview_handler<T, Svc, Auth>(
    State(state): State<ProjectRouterState<T, Svc, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOnly>,
    project_access: ProjectAccessLevelExtractor<ViewAccessLevel, Svc, Auth>,
    company_access: MacroUserTeamExtractorV2<ReadProjectWorkScoped, Svc, Auth>,
    query: Result<Query<GetProjectOverviewQuery>, QueryRejection>,
) -> Result<Json<GetProjectOverviewResponse>, ProjectError>
where
    T: ProjectService,
    Svc: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let Query(query) =
        query.map_err(|_| ProjectError::BadRequest("invalid asOfDate".to_owned()))?;
    let as_of_date = NaiveDate::parse_from_str(&query.as_of_date, "%Y-%m-%d")
        .ok()
        .filter(|date| date.format("%Y-%m-%d").to_string() == query.as_of_date)
        .ok_or_else(|| ProjectError::BadRequest("invalid asOfDate".to_owned()))?;
    let overview = state
        .service
        .get_project_overview(
            project_access.entity_access_receipt,
            company_access.entity_access_receipt,
            as_of_date,
        )
        .await?;

    tracing::debug!(user_id = %user.authorization.macro_user_id, "read project overview");
    Ok(Json(GetProjectOverviewResponse {
        error: false,
        data: overview,
    }))
}
