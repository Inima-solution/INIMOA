//! Direct-human company business role grant/revoke behind a reauthentication
//! receipt.
//!
//! The adapter only extracts the authenticated principal, team, request
//! correlation, and body, then delegates to the teams-owned
//! `PgBusinessRoleChangeService`. Team and actor never come from the body,
//! and receipt identifiers, human reasons, and database details never leave
//! the service boundary in responses or logs.

use axum::{
    Extension, Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use business_audit::{AuditReason, RequestCorrelationId};
use entity_access::{
    domain::models::MemberTeamRole, inbound::axum_extractors::MacroUserTeamExtractorV2,
};
use macro_authorization::{MacroAuthorizationExtractor, UserOnly};
use macro_user_id::user_id::MacroUserIdStr;
use model::response::ErrorResponse;
use models_team::BusinessRole;
use std::borrow::Cow;
use teams::domain::business_role_change::{
    BusinessRoleChangeOutcome, GrantBusinessRoleCommand, RevokeBusinessRoleCommand,
    RoleChangeDenialReason,
};
use teams::outbound::business_role_change::PgBusinessRoleChangeService;
use tower_http::request_id::RequestId;
use uuid::Uuid;

use super::context::{ApiContext, AuthorizationService, EntityAccessServiceType};

/// Request body for a company business role change.
///
/// The acting team and actor come from the authenticated extractor receipts;
/// they must never be supplied in the body.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct BusinessRoleChangeRequest {
    /// Canonical principal of the direct human target.
    target: String,
    /// Company business role to grant or revoke.
    business_role: BusinessRole,
    /// One-time reauthentication receipt authorizing the change.
    reauthentication_receipt: Uuid,
    /// Human rationale recorded on the success audit event.
    reason: String,
}

/// Minimal confirmation of an applied company business role change.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct BusinessRoleChangeResponse {
    /// Whether the role was granted or revoked.
    result: &'static str,
    /// Business role that changed.
    business_role: BusinessRole,
    /// Principal that changed.
    target: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RoleChangeError {
    /// A request field failed validation before the audited service boundary.
    #[error("invalid business role change request")]
    InvalidRequest,
    /// The authentication and team-access receipts disagree on the principal.
    #[error("direct user authentication required")]
    PrincipalMismatch,
    /// An audited denial was recorded. The response carries the machine code.
    #[error("business role change denied")]
    Denied {
        /// Closed machine denial reason.
        reason: RoleChangeDenialReason,
    },
    /// The audited change failed inside the database boundary.
    #[error("internal server error")]
    Internal,
}

impl IntoResponse for RoleChangeError {
    fn into_response(self) -> Response {
        let (status, message): (StatusCode, Cow<'static, str>) = match self {
            Self::InvalidRequest => (
                StatusCode::BAD_REQUEST,
                "invalid business role change request".into(),
            ),
            Self::PrincipalMismatch => (
                StatusCode::FORBIDDEN,
                "direct user authentication required".into(),
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error".into(),
            ),
            Self::Denied { reason } => (denial_status(reason), reason.as_str().into()),
        };
        (status, Json(ErrorResponse { message })).into_response()
    }
}

/// Single status mapping for the closed denial vocabulary.
fn denial_status(reason: RoleChangeDenialReason) -> StatusCode {
    match reason {
        RoleChangeDenialReason::InvalidReceipt
        | RoleChangeDenialReason::AlreadyGranted
        | RoleChangeDenialReason::NotGranted => StatusCode::CONFLICT,
        RoleChangeDenialReason::InsufficientGovernance => StatusCode::FORBIDDEN,
        RoleChangeDenialReason::TargetNotMember => StatusCode::NOT_FOUND,
        RoleChangeDenialReason::SelfGrant
        | RoleChangeDenialReason::MemberIsDerived
        | RoleChangeDenialReason::AgentRequiresAgentFlow => StatusCode::BAD_REQUEST,
    }
}

/// Builds the company business role change routes.
pub fn router() -> Router<ApiContext> {
    Router::new()
        .route("/business-role/grant", post(grant_handler))
        .route("/business-role/revoke", post(revoke_handler))
}

/// Grants one company business role to a direct human teammate.
#[utoipa::path(
    post,
    operation_id = "grant_team_business_role",
    path = "/team/business-role/grant",
    request_body = BusinessRoleChangeRequest,
    responses(
        (status = 200, body = BusinessRoleChangeResponse),
        (status = 400, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn grant_handler(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<MemberTeamRole, EntityAccessServiceType, AuthorizationService>,
    Extension(request_id): Extension<RequestId>,
    State(ctx): State<ApiContext>,
    Json(request): Json<BusinessRoleChangeRequest>,
) -> Result<Json<BusinessRoleChangeResponse>, RoleChangeError> {
    apply(
        true,
        &user,
        &access,
        request_id.header_value(),
        &ctx,
        &request,
    )
    .await
}

/// Revokes one company business role from a direct human teammate.
#[utoipa::path(
    post,
    operation_id = "revoke_team_business_role",
    path = "/team/business-role/revoke",
    request_body = BusinessRoleChangeRequest,
    responses(
        (status = 200, body = BusinessRoleChangeResponse),
        (status = 400, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn revoke_handler(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<MemberTeamRole, EntityAccessServiceType, AuthorizationService>,
    Extension(request_id): Extension<RequestId>,
    State(ctx): State<ApiContext>,
    Json(request): Json<BusinessRoleChangeRequest>,
) -> Result<Json<BusinessRoleChangeResponse>, RoleChangeError> {
    apply(
        false,
        &user,
        &access,
        request_id.header_value(),
        &ctx,
        &request,
    )
    .await
}

/// Derives actor/team from the receipts, validates the body before the
/// audited service boundary, and delegates to the teams-owned service.
async fn apply(
    grant: bool,
    user: &MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: &MacroUserTeamExtractorV2<
        MemberTeamRole,
        EntityAccessServiceType,
        AuthorizationService,
    >,
    request_id: &axum::http::HeaderValue,
    ctx: &ApiContext,
    request: &BusinessRoleChangeRequest,
) -> Result<Json<BusinessRoleChangeResponse>, RoleChangeError> {
    let actor = user.authorization.macro_user_id.clone();
    let access_principal = access
        .entity_access_receipt
        .get_authenticated_user()
        .map_err(|_| RoleChangeError::PrincipalMismatch)?;
    if access_principal != &actor {
        return Err(RoleChangeError::PrincipalMismatch);
    }
    let team_id = access
        .entity_access_receipt
        .entity()
        .entity_id
        .parse()
        .map_err(|_| RoleChangeError::Internal)?;

    let (target, correlation, reason) = validate(request, request_id)?;
    let service = PgBusinessRoleChangeService::new(ctx.db.clone());
    let outcome = if grant {
        service
            .grant(&GrantBusinessRoleCommand {
                team_id,
                actor,
                target: target.clone(),
                business_role: request.business_role,
                receipt_id: request.reauthentication_receipt,
                request_id: correlation,
                reason,
            })
            .await
    } else {
        service
            .revoke(&RevokeBusinessRoleCommand {
                team_id,
                actor,
                target: target.clone(),
                business_role: request.business_role,
                receipt_id: request.reauthentication_receipt,
                request_id: correlation,
                reason,
            })
            .await
    }
    .map_err(|_| RoleChangeError::Internal)?;

    let result = match outcome {
        BusinessRoleChangeOutcome::Granted => "granted",
        BusinessRoleChangeOutcome::Revoked => "revoked",
        BusinessRoleChangeOutcome::Denied(reason) => {
            return Err(RoleChangeError::Denied { reason });
        }
    };
    Ok(Json(BusinessRoleChangeResponse {
        result,
        business_role: request.business_role,
        target: request.target.clone(),
    }))
}

/// Validates every body field before the audited service boundary so bad
/// input never reaches the receipt claim or the denial ledger.
fn validate(
    request: &BusinessRoleChangeRequest,
    request_id: &axum::http::HeaderValue,
) -> Result<(MacroUserIdStr<'static>, RequestCorrelationId, AuditReason), RoleChangeError> {
    let target = MacroUserIdStr::try_from(request.target.clone())
        .map_err(|_| RoleChangeError::InvalidRequest)?;
    let request_id = request_id
        .to_str()
        .ok()
        .and_then(|value| RequestCorrelationId::try_new(value).ok())
        .ok_or(RoleChangeError::InvalidRequest)?;
    let reason = AuditReason::try_new(request.reason.clone())
        .map_err(|_| RoleChangeError::InvalidRequest)?;
    Ok((target, request_id, reason))
}

#[cfg(test)]
mod test {
    use http_body_util::BodyExt;

    use super::*;

    async fn status_and_body(response: Response) -> (StatusCode, serde_json::Value) {
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn denials_map_to_the_closed_status_and_code() {
        let expected: [(RoleChangeDenialReason, StatusCode, &str); 8] = [
            (
                RoleChangeDenialReason::InvalidReceipt,
                StatusCode::CONFLICT,
                "invalid_receipt",
            ),
            (
                RoleChangeDenialReason::InsufficientGovernance,
                StatusCode::FORBIDDEN,
                "insufficient_governance",
            ),
            (
                RoleChangeDenialReason::TargetNotMember,
                StatusCode::NOT_FOUND,
                "target_not_member",
            ),
            (
                RoleChangeDenialReason::MemberIsDerived,
                StatusCode::BAD_REQUEST,
                "member_is_derived",
            ),
            (
                RoleChangeDenialReason::AgentRequiresAgentFlow,
                StatusCode::BAD_REQUEST,
                "agent_requires_agent_flow",
            ),
            (
                RoleChangeDenialReason::SelfGrant,
                StatusCode::BAD_REQUEST,
                "self_grant",
            ),
            (
                RoleChangeDenialReason::AlreadyGranted,
                StatusCode::CONFLICT,
                "already_granted",
            ),
            (
                RoleChangeDenialReason::NotGranted,
                StatusCode::CONFLICT,
                "not_granted",
            ),
        ];
        for (reason, status, code) in expected {
            let response = RoleChangeError::Denied { reason }.into_response();
            let (actual, body) = status_and_body(response).await;
            assert_eq!(actual, status, "status for {reason:?}");
            assert_eq!(body["message"], code);
        }
    }

    #[tokio::test]
    async fn pre_service_failures_stay_outside_the_denial_codes() {
        let (status, body) = status_and_body(RoleChangeError::InvalidRequest.into_response()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], "invalid business role change request");

        let (status, body) =
            status_and_body(RoleChangeError::PrincipalMismatch.into_response()).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["message"], "direct user authentication required");

        let (status, body) = status_and_body(RoleChangeError::Internal.into_response()).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["message"], "internal server error");
    }

    fn request(target: &str, reason: &str) -> BusinessRoleChangeRequest {
        BusinessRoleChangeRequest {
            target: target.into(),
            business_role: BusinessRole::Manager,
            reauthentication_receipt: Uuid::new_v4(),
            reason: reason.into(),
        }
    }

    fn header_value(value: &str) -> axum::http::HeaderValue {
        value.parse().unwrap()
    }

    #[test]
    fn validate_accepts_bounded_inputs() {
        let (target, correlation, reason) = validate(
            &request("macro|user@example.com", "coverage"),
            &header_value("req-1"),
        )
        .unwrap();
        assert_eq!(target.as_ref(), "macro|user@example.com");
        assert_eq!(correlation.as_ref(), "req-1");
        assert_eq!(reason.as_ref(), "coverage");
    }

    #[test]
    fn validate_rejects_unbounded_or_malformed_inputs() {
        let cases: [(&str, &str, &str); 5] = [
            // Malformed target principal.
            ("not-a-principal", "coverage", "req-1"),
            // Empty reason.
            ("macro|user@example.com", "   ", "req-1"),
            // Reason past the audit bound.
            ("macro|user@example.com", &"x".repeat(1001), "req-1"),
            // Empty request correlation.
            ("macro|user@example.com", "coverage", "  "),
            // Request correlation past the audit bound.
            ("macro|user@example.com", "coverage", &"x".repeat(257)),
        ];
        for (target, reason, request_id) in cases {
            assert!(
                validate(&request(target, reason), &header_value(request_id)).is_err(),
                "target={target} request_id_len={}",
                request_id.len()
            );
        }
    }

    #[test]
    fn success_response_carries_no_receipt_or_reason() {
        let response = BusinessRoleChangeResponse {
            result: "granted",
            business_role: BusinessRole::Manager,
            target: "macro|user@example.com".into(),
        };
        let body = serde_json::to_value(&response).unwrap();
        assert_eq!(body["result"], "granted");
        assert_eq!(body["business_role"], "manager");
        assert_eq!(body["target"], "macro|user@example.com");
        assert_eq!(body.as_object().unwrap().len(), 3);
        assert!(body.get("reauthentication_receipt").is_none());
        assert!(body.get("receipt").is_none());
        assert!(body.get("reason").is_none());
    }

    #[test]
    fn grant_handler_orders_user_only_before_member_team_role() {
        let source = include_str!("business_role_change.rs");
        let start = source
            .find("pub(crate) async fn grant_handler(")
            .expect("grant handler");
        let end = source[start..]
            .find(") -> Result<")
            .expect("handler signature");
        let signature = &source[start..start + end];
        let user_only = signature
            .find("MacroAuthorizationExtractor<AuthorizationService, UserOnly>")
            .expect("UserOnly extractor");
        let member = signature
            .find("MacroUserTeamExtractorV2<MemberTeamRole")
            .expect("MemberTeamRole extractor");
        assert!(user_only < member);
    }
}
