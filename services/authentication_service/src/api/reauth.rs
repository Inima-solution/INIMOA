//! Human step-up authentication for sensitive team operations.

use std::time::Duration;

use axum::{
    Extension, Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use entity_access::{
    domain::models::AdminTeamRole, inbound::axum_extractors::MacroUserTeamExtractorV2,
};
use fusionauth::{
    error::FusionAuthClientError,
    password::{MultiFactorChallenge, MultiFactorMethod, PasswordVerification},
};
use macro_authorization::{MacroAuthorizationExtractor, UserOnly};
use macro_user_id::email::ReadEmailParts;
use model::response::ErrorResponse;
use rate_limit::{RateLimitConfig, RateLimitKey, RateLimitService};
use reauthentication::{
    PgReauthenticationReceiptRepo, ProofMethod, ReauthenticationReceipt,
    ReauthenticationReceiptRepo, ReceiptPurpose, ReceiptScope, RequestCorrelationId,
};
use tower_http::request_id::RequestId;
use uuid::Uuid;
use zeroize::Zeroize;

use super::context::{ApiContext, AuthorizationService, EntityAccessServiceType};

const MAX_PASSWORD_BYTES: usize = 1024;

/// Request body for password step-up authentication.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ReauthenticateRequest {
    /// Current FusionAuth password. It is zeroed when the request is dropped.
    password: String,
}

impl Drop for ReauthenticateRequest {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

/// Opaque one-time receipt returned after successful step-up authentication.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ReauthenticateResponse {
    /// Random bearer receipt identifier.
    reauthentication_receipt: Uuid,
    /// Seconds until the receipt expires.
    expires_in: u64,
}

#[derive(Debug, serde::Serialize)]
struct MultiFactorRequiredResponse {
    message: &'static str,
    code: &'static str,
    two_factor_id: String,
    methods: Vec<MultiFactorMethod>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReauthenticateError {
    #[error("password must contain between 1 and {MAX_PASSWORD_BYTES} bytes")]
    MalformedPassword,
    #[error("direct user authentication required")]
    DirectUserRequired,
    #[error("reauthentication failed")]
    InvalidCredentials,
    #[error("multi-factor authentication required")]
    MultiFactorRequired(MultiFactorChallenge),
    #[error("too many reauthentication attempts")]
    RateLimited,
    #[error("authentication provider unavailable")]
    UpstreamUnavailable,
    #[error("internal server error")]
    Internal,
}

impl IntoResponse for ReauthenticateError {
    fn into_response(self) -> Response {
        if let Self::MultiFactorRequired(challenge) = self {
            return (
                StatusCode::UNAUTHORIZED,
                Json(MultiFactorRequiredResponse {
                    message: "multi-factor authentication required",
                    code: "mfa_required",
                    two_factor_id: challenge.two_factor_id,
                    methods: challenge.methods,
                }),
            )
                .into_response();
        }

        let status = match self {
            Self::MalformedPassword => StatusCode::BAD_REQUEST,
            Self::DirectUserRequired => StatusCode::FORBIDDEN,
            Self::InvalidCredentials => StatusCode::UNAUTHORIZED,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::UpstreamUnavailable => StatusCode::BAD_GATEWAY,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::MultiFactorRequired(_) => unreachable!(),
        };
        (
            status,
            Json(ErrorResponse {
                message: self.to_string().into(),
            }),
        )
            .into_response()
    }
}

/// Builds the step-up authentication route.
pub fn router() -> Router<ApiContext> {
    Router::new().route("/reauth", post(handler))
}

/// Verifies a directly authenticated team owner/admin and mints a five-minute receipt.
#[utoipa::path(
    post,
    operation_id = "reauthenticate_for_team_role_change",
    path = "/team/reauth",
    request_body = ReauthenticateRequest,
    responses(
        (status = 200, body = ReauthenticateResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, description = "Invalid password or MFA required"),
        (status = 403, body = ErrorResponse),
        (status = 429, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
        (status = 502, body = ErrorResponse),
    )
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn handler(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<AdminTeamRole, EntityAccessServiceType, AuthorizationService>,
    Extension(request_id): Extension<RequestId>,
    State(ctx): State<ApiContext>,
    Json(request): Json<ReauthenticateRequest>,
) -> Result<Json<ReauthenticateResponse>, ReauthenticateError> {
    if request.password.is_empty() || request.password.len() > MAX_PASSWORD_BYTES {
        return Err(ReauthenticateError::MalformedPassword);
    }

    let access_principal = access
        .entity_access_receipt
        .get_authenticated_user()
        .map_err(|_| ReauthenticateError::DirectUserRequired)?;
    let principal = user.authorization.macro_user_id;
    if access_principal != &principal {
        return Err(ReauthenticateError::DirectUserRequired);
    }
    let team_id = access
        .entity_access_receipt
        .entity()
        .entity_id
        .parse()
        .map_err(|_| ReauthenticateError::Internal)?;

    let rate_limit_key = RateLimitKey::builder(&"team-reauthentication")
        .append(&principal.as_ref())
        .finish();
    match ctx
        .rate_limit_service
        .check_rate_limit(
            rate_limit_key,
            RateLimitConfig::new(10, Duration::from_secs(15 * 60)),
        )
        .await
        .map_err(|error| {
            tracing::error!(?error, "reauthentication rate-limit check failed");
            ReauthenticateError::Internal
        })? {
        Ok(_) => {}
        Err(_) => return Err(ReauthenticateError::RateLimited),
    }

    match ctx
        .auth_client
        .verify_password(principal.email_part().email_str(), &request.password)
        .await
    {
        Ok(PasswordVerification::Verified) => {}
        Ok(PasswordVerification::MultiFactorRequired(challenge)) => {
            return Err(ReauthenticateError::MultiFactorRequired(challenge));
        }
        Err(
            FusionAuthClientError::IncorrectCredentials
            | FusionAuthClientError::UserNotRegistered
            | FusionAuthClientError::UserNotVerified
            | FusionAuthClientError::UserRegistrationNotVerified
            | FusionAuthClientError::PasswordChangeRequired
            | FusionAuthClientError::LoginPrevented,
        ) => return Err(ReauthenticateError::InvalidCredentials),
        Err(error) => {
            tracing::error!(?error, "fusionauth password verification failed");
            return Err(ReauthenticateError::UpstreamUnavailable);
        }
    }

    let request_id = request_id
        .header_value()
        .to_str()
        .map_err(|_| ReauthenticateError::Internal)?;
    let receipt = ReauthenticationReceipt::issue(
        ReceiptScope::new(team_id, principal, ReceiptPurpose::CompanyRoleChange),
        ProofMethod::Password,
        chrono::Utc::now(),
        RequestCorrelationId::try_new(request_id).map_err(|_| ReauthenticateError::Internal)?,
    );
    PgReauthenticationReceiptRepo::new(ctx.db.clone())
        .mint(&receipt)
        .await
        .map_err(|error| {
            tracing::error!(?error, "unable to store reauthentication receipt");
            ReauthenticateError::Internal
        })?;

    Ok(Json(ReauthenticateResponse {
        reauthentication_receipt: receipt.id,
        expires_in: 300,
    }))
}

#[cfg(test)]
mod test {
    use http_body_util::BodyExt;

    use super::*;

    #[test]
    fn error_statuses_fail_closed() {
        for (error, expected) in [
            (
                ReauthenticateError::MalformedPassword,
                StatusCode::BAD_REQUEST,
            ),
            (
                ReauthenticateError::DirectUserRequired,
                StatusCode::FORBIDDEN,
            ),
            (
                ReauthenticateError::InvalidCredentials,
                StatusCode::UNAUTHORIZED,
            ),
            (
                ReauthenticateError::RateLimited,
                StatusCode::TOO_MANY_REQUESTS,
            ),
            (
                ReauthenticateError::UpstreamUnavailable,
                StatusCode::BAD_GATEWAY,
            ),
            (
                ReauthenticateError::Internal,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ] {
            assert_eq!(error.into_response().status(), expected);
        }
    }

    #[tokio::test]
    async fn mfa_response_preserves_only_the_challenge_contract() {
        let response = ReauthenticateError::MultiFactorRequired(MultiFactorChallenge {
            two_factor_id: "challenge-id".into(),
            methods: vec![MultiFactorMethod {
                id: "TOTP".into(),
                method: "authenticator".into(),
            }],
        })
        .into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["code"], "mfa_required");
        assert_eq!(body["two_factor_id"], "challenge-id");
        assert_eq!(body["methods"][0]["method"], "authenticator");
        assert!(body.get("password").is_none());
    }
}
