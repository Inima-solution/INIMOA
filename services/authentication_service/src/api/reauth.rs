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
use macro_user_id::user_id::MacroUserIdStr;
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
const MAX_MFA_CHALLENGE_BYTES: usize = 256;
const MAX_MFA_CODE_BYTES: usize = 1024;

/// Request body for password step-up authentication.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ReauthenticateRequest {
    /// Current FusionAuth password. It is zeroed when the request is dropped.
    password: String,
}

/// Request body for completing an existing MFA step-up challenge.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ReauthenticateMfaRequest {
    /// The short-lived FusionAuth challenge identifier.
    two_factor_id: String,
    /// MFA code. It is zeroed when the request is dropped.
    code: String,
}

impl Drop for ReauthenticateMfaRequest {
    fn drop(&mut self) {
        self.code.zeroize();
    }
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

/// A safe MFA method projection exposed by the reauthentication API.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ReauthenticateMfaMethod {
    /// FusionAuth method identifier.
    id: String,
    /// Method kind, such as `authenticator`, `email`, or `sms`.
    method: String,
}

/// Typed unauthorized outcomes from the password step.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ReauthenticateUnauthorizedResponse {
    /// The supplied password was not accepted.
    InvalidCredentials { message: &'static str },
    /// A challenge must be completed before a receipt can be minted.
    MfaRequired {
        message: &'static str,
        two_factor_id: String,
        methods: Vec<ReauthenticateMfaMethod>,
    },
}

/// Typed unauthorized outcome from the MFA completion step.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ReauthenticateMfaUnauthorizedResponse {
    /// The submitted MFA challenge or proof was not accepted.
    InvalidMfa { message: &'static str },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReauthenticateError {
    #[error("password must contain between 1 and {MAX_PASSWORD_BYTES} bytes")]
    MalformedPassword,
    #[error("MFA challenge and code must be nonempty and within their size limits")]
    MalformedMfa,
    #[error("direct user authentication required")]
    DirectUserRequired,
    #[error("reauthentication failed")]
    InvalidCredentials,
    #[error("multi-factor authentication required")]
    MultiFactorRequired(MultiFactorChallenge),
    #[error("multi-factor authentication failed")]
    InvalidMfa,
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
                Json(ReauthenticateUnauthorizedResponse::MfaRequired {
                    message: "multi-factor authentication required",
                    two_factor_id: challenge.two_factor_id,
                    methods: mfa_methods(challenge.methods),
                }),
            )
                .into_response();
        }

        if matches!(self, Self::InvalidCredentials) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ReauthenticateUnauthorizedResponse::InvalidCredentials {
                    message: "reauthentication failed",
                }),
            )
                .into_response();
        }

        if matches!(self, Self::InvalidMfa) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ReauthenticateMfaUnauthorizedResponse::InvalidMfa {
                    message: "multi-factor authentication failed",
                }),
            )
                .into_response();
        }

        let status = match self {
            Self::MalformedPassword => StatusCode::BAD_REQUEST,
            Self::MalformedMfa => StatusCode::BAD_REQUEST,
            Self::DirectUserRequired => StatusCode::FORBIDDEN,
            Self::InvalidCredentials => StatusCode::UNAUTHORIZED,
            Self::InvalidMfa => StatusCode::UNAUTHORIZED,
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

fn mfa_methods(methods: Vec<MultiFactorMethod>) -> Vec<ReauthenticateMfaMethod> {
    methods
        .into_iter()
        .map(|method| ReauthenticateMfaMethod {
            id: method.id,
            method: method.method,
        })
        .collect()
}

fn valid_mfa_request_shape(request: &ReauthenticateMfaRequest) -> bool {
    !request.two_factor_id.is_empty()
        && request.two_factor_id.len() <= MAX_MFA_CHALLENGE_BYTES
        && !request.code.is_empty()
        && request.code.len() <= MAX_MFA_CODE_BYTES
}

fn authenticated_email_matches_principal(
    authenticated_email: &str,
    principal: &MacroUserIdStr<'static>,
) -> bool {
    authenticated_email.eq_ignore_ascii_case(principal.email_part().email_str())
}

/// Builds the step-up authentication route.
pub fn router() -> Router<ApiContext> {
    Router::new()
        .route("/reauth", post(handler))
        .route("/reauth/mfa", post(mfa_handler))
}

async fn check_rate_limit(
    ctx: &ApiContext,
    principal: &MacroUserIdStr<'static>,
) -> Result<(), ReauthenticateError> {
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
        Ok(_) => Ok(()),
        Err(_) => Err(ReauthenticateError::RateLimited),
    }
}

fn team_scope(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<AdminTeamRole, EntityAccessServiceType, AuthorizationService>,
) -> Result<(Uuid, MacroUserIdStr<'static>), ReauthenticateError> {
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
    Ok((team_id, principal))
}

async fn mint_receipt(
    ctx: &ApiContext,
    request_id: RequestId,
    team_id: Uuid,
    principal: MacroUserIdStr<'static>,
    purpose: ReceiptPurpose,
    proof_method: ProofMethod,
) -> Result<Json<ReauthenticateResponse>, ReauthenticateError> {
    let request_id = request_id
        .header_value()
        .to_str()
        .map_err(|_| ReauthenticateError::Internal)?;
    let receipt = ReauthenticationReceipt::issue(
        ReceiptScope::new(team_id, principal, purpose),
        proof_method,
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

/// Verifies a password step-up proof and mints a receipt scoped to the explicit purpose.
pub(crate) async fn reauthenticate_password_for_purpose(
    ctx: &ApiContext,
    request_id: RequestId,
    team_id: Uuid,
    principal: MacroUserIdStr<'static>,
    request: &ReauthenticateRequest,
    purpose: ReceiptPurpose,
) -> Result<Json<ReauthenticateResponse>, ReauthenticateError> {
    if request.password.is_empty() || request.password.len() > MAX_PASSWORD_BYTES {
        return Err(ReauthenticateError::MalformedPassword);
    }

    check_rate_limit(ctx, &principal).await?;
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

    mint_receipt(
        ctx,
        request_id,
        team_id,
        principal,
        purpose,
        ProofMethod::Password,
    )
    .await
}

/// Verifies an MFA step-up proof and mints a receipt scoped to the explicit purpose.
pub(crate) async fn reauthenticate_mfa_for_purpose(
    ctx: &ApiContext,
    request_id: RequestId,
    team_id: Uuid,
    principal: MacroUserIdStr<'static>,
    request: &ReauthenticateMfaRequest,
    purpose: ReceiptPurpose,
) -> Result<Json<ReauthenticateResponse>, ReauthenticateError> {
    if !valid_mfa_request_shape(request) {
        return Err(ReauthenticateError::MalformedMfa);
    }

    check_rate_limit(ctx, &principal).await?;
    let authenticated_email = ctx
        .auth_client
        .verify_multi_factor(&request.two_factor_id, &request.code)
        .await
        .map_err(|error| match error {
            FusionAuthClientError::IncorrectCode => ReauthenticateError::InvalidMfa,
            _ => {
                tracing::error!(?error, "fusionauth MFA verification failed");
                ReauthenticateError::UpstreamUnavailable
            }
        })?;
    if !authenticated_email_matches_principal(&authenticated_email, &principal) {
        return Err(ReauthenticateError::InvalidMfa);
    }

    mint_receipt(
        ctx,
        request_id,
        team_id,
        principal,
        purpose,
        ProofMethod::PasswordMfa,
    )
    .await
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
        (status = 401, body = ReauthenticateUnauthorizedResponse),
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

    let (team_id, principal) = team_scope(user, access)?;
    reauthenticate_password_for_purpose(
        &ctx,
        request_id,
        team_id,
        principal,
        &request,
        ReceiptPurpose::CompanyRoleChange,
    )
    .await
}

/// Completes an existing MFA challenge for the same directly authenticated team owner/admin.
#[utoipa::path(
    post,
    operation_id = "complete_team_reauthentication_mfa",
    path = "/team/reauth/mfa",
    request_body = ReauthenticateMfaRequest,
    responses(
        (status = 200, body = ReauthenticateResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ReauthenticateMfaUnauthorizedResponse),
        (status = 403, body = ErrorResponse),
        (status = 429, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
        (status = 502, body = ErrorResponse),
    )
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn mfa_handler(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<AdminTeamRole, EntityAccessServiceType, AuthorizationService>,
    Extension(request_id): Extension<RequestId>,
    State(ctx): State<ApiContext>,
    Json(request): Json<ReauthenticateMfaRequest>,
) -> Result<Json<ReauthenticateResponse>, ReauthenticateError> {
    if !valid_mfa_request_shape(&request) {
        return Err(ReauthenticateError::MalformedMfa);
    }

    let (team_id, principal) = team_scope(user, access)?;
    reauthenticate_mfa_for_purpose(
        &ctx,
        request_id,
        team_id,
        principal,
        &request,
        ReceiptPurpose::CompanyRoleChange,
    )
    .await
}

#[cfg(test)]
mod test {
    use http_body_util::BodyExt;

    use super::*;

    fn mfa_request(two_factor_id: String, code: String) -> ReauthenticateMfaRequest {
        ReauthenticateMfaRequest {
            two_factor_id,
            code,
        }
    }

    fn principal(value: &str) -> MacroUserIdStr<'static> {
        MacroUserIdStr::try_from(value.to_owned()).unwrap()
    }

    #[test]
    fn error_statuses_fail_closed() {
        for (error, expected) in [
            (
                ReauthenticateError::MalformedPassword,
                StatusCode::BAD_REQUEST,
            ),
            (ReauthenticateError::MalformedMfa, StatusCode::BAD_REQUEST),
            (
                ReauthenticateError::DirectUserRequired,
                StatusCode::FORBIDDEN,
            ),
            (
                ReauthenticateError::InvalidCredentials,
                StatusCode::UNAUTHORIZED,
            ),
            (ReauthenticateError::InvalidMfa, StatusCode::UNAUTHORIZED),
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

    #[tokio::test]
    async fn unauthorized_responses_publish_stable_machine_codes() {
        for (error, expected_code) in [
            (
                ReauthenticateError::InvalidCredentials,
                "invalid_credentials",
            ),
            (ReauthenticateError::InvalidMfa, "invalid_mfa"),
        ] {
            let response = error.into_response();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["code"], expected_code);
        }
    }

    #[test]
    fn mfa_request_shape_requires_bounded_challenge_and_code() {
        assert!(valid_mfa_request_shape(&mfa_request(
            "challenge-id".into(),
            "123456".into()
        )));
        for request in [
            mfa_request(String::new(), "123456".into()),
            mfa_request("challenge-id".into(), String::new()),
            mfa_request("x".repeat(MAX_MFA_CHALLENGE_BYTES + 1), "123456".into()),
            mfa_request("challenge-id".into(), "x".repeat(MAX_MFA_CODE_BYTES + 1)),
        ] {
            assert!(!valid_mfa_request_shape(&request));
        }
    }

    #[test]
    fn mfa_provider_email_must_match_the_direct_principal() {
        let actor = principal("macro|actor@example.com");
        assert!(authenticated_email_matches_principal(
            "ACTOR@EXAMPLE.COM",
            &actor
        ));
        assert!(!authenticated_email_matches_principal(
            "other@example.com",
            &actor
        ));
    }
}
