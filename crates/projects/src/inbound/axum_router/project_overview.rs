//! Typed REST handler for the bounded project overview.

use axum::{Json, extract::State};
use entity_access::{
    domain::{
        models::{ReadProjectWorkScoped, ViewAccessLevel},
        ports::EntityAccessService,
    },
    inbound::axum_extractors::{MacroUserTeamExtractorV2, ProjectAccessLevelExtractor},
};
use macro_authorization::{MacroAuthorizationExtractor, MacroAuthorizationService, UserOnly};
use model::response::TypedSuccessResponse;

use super::ProjectRouterState;
use crate::domain::{
    models::{ProjectError, ProjectOverview},
    ports::ProjectService,
};

/// Successful bounded project-overview response.
pub type GetProjectOverviewResponse = TypedSuccessResponse<ProjectOverview>;

/// Read the canonical, bounded overview for one project.
#[utoipa::path(
    tag = "project",
    get,
    operation_id = "get_project_overview",
    path = "/v2/projects/{id}/overview",
    params(("id" = String, Path, description = "ID of the project")),
    responses(
        (status = 200, body = inline(GetProjectOverviewResponse)),
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
) -> Result<Json<GetProjectOverviewResponse>, ProjectError>
where
    T: ProjectService,
    Svc: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let overview = state
        .service
        .get_project_overview(
            project_access.entity_access_receipt,
            company_access.entity_access_receipt,
        )
        .await?;

    tracing::debug!(user_id = %user.authorization.macro_user_id, "read project overview");
    Ok(Json(GetProjectOverviewResponse {
        error: false,
        data: overview,
    }))
}
