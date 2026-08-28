use axum::{
    Extension,
    extract::{Path, State},
};
use business_audit::RequestCorrelationId;
use entity_access::{
    domain::{models::AdminTeamRole, ports::EntityAccessService},
    inbound::axum_extractors::MacroUserTeamExtractorV2,
};
use macro_authorization::MacroAuthorizationService;
use macro_user_id::user_id::MacroUserIdStr;
use model_error_response::ErrorResponse;
use tower_http::request_id::RequestId;

use crate::domain::{model::RemoveUserFromTeamError, team_repo::TeamService};

use super::TeamRouterState;

/// Path parameters for remove user endpoint.
#[derive(serde::Deserialize)]
pub struct Param {
    /// The ID of the user to remove.
    pub remove_user_id: MacroUserIdStr<'static>,
}

/// Removes a user from a team.
#[utoipa::path(
    delete,
    path = "/team/remove/{remove_user_id}",
    operation_id = "remove_user_from_team",
    params(
        ("remove_user_id" = String, Path, description = "The ID of the user to remove")
    ),
    responses(
        (status = 200),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    ),
)]
#[tracing::instrument(skip_all, err)]
pub async fn handler<T: TeamService, Eas: EntityAccessService, Auth: MacroAuthorizationService>(
    access: MacroUserTeamExtractorV2<AdminTeamRole, Eas, Auth>,
    Extension(request_id): Extension<RequestId>,
    State(state): State<TeamRouterState<T, Eas, Auth>>,
    Path(Param { remove_user_id }): Path<Param>,
) -> Result<(), RemoveUserFromTeamError> {
    state
        .service
        .remove_user_from_team_with_request_id(
            access.entity_access_receipt,
            &remove_user_id,
            RequestCorrelationId::try_new(request_id.header_value().to_str().map_err(|_| {
                RemoveUserFromTeamError::TeamError(
                    crate::domain::model::TeamError::StorageLayerError(anyhow::anyhow!(
                        "invalid request correlation"
                    )),
                )
            })?)
            .map_err(|_| {
                RemoveUserFromTeamError::TeamError(
                    crate::domain::model::TeamError::StorageLayerError(anyhow::anyhow!(
                        "invalid request correlation"
                    )),
                )
            })?,
        )
        .await?;
    Ok(())
}
