use crate::api::{
    context::ApiContext,
    utils::{create_access_token_cookie, create_refresh_token_cookie},
};
use axum::{
    Json,
    extract::{self, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use fusionauth::error::FusionAuthClientError;
use macro_middleware::tracking::ClientIp;
use model::{
    authentication::login::request::PasswordRequest,
    response::{ErrorResponse, UserTokensResponse},
};
use tower_cookies::Cookies;

fn password_login_error(error: FusionAuthClientError) -> Response {
    let (status, message) = match error {
        FusionAuthClientError::UserNotVerified => (
            StatusCode::UNAUTHORIZED,
            "user has not verified their primary email",
        ),
        FusionAuthClientError::UserRegistrationNotVerified => (
            StatusCode::UNAUTHORIZED,
            "user registration has not been verified",
        ),
        FusionAuthClientError::IncorrectCredentials | FusionAuthClientError::LoginPrevented => {
            (StatusCode::UNAUTHORIZED, "unable to login user")
        }
        FusionAuthClientError::PasswordChangeRequired => {
            (StatusCode::UNAUTHORIZED, "password change required")
        }
        FusionAuthClientError::MultiFactorAuthenticationRequired => (
            StatusCode::UNAUTHORIZED,
            "multi-factor authentication required",
        ),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "unable to login user"),
    };
    (
        status,
        Json(ErrorResponse {
            message: message.into(),
        }),
    )
        .into_response()
}

/// Performs a password login
#[utoipa::path(
        post,
        operation_id = "password_login",
        path = "/login/password",
        responses(
            (status = 200, body=UserTokensResponse),
            (status = 400, body=ErrorResponse),
            (status = 401, body=ErrorResponse),
            (status = 500, body=ErrorResponse),
        )
    )]
#[tracing::instrument(skip(ctx, req, ip_context), fields(email=%req.email, client_ip=%ip_context), err(Debug))]
pub async fn handler(
    State(ctx): State<ApiContext>,
    ip_context: ClientIp,
    cookies: Cookies,
    extract::Json(req): extract::Json<PasswordRequest>,
) -> Result<Response, Response> {
    if !email_validator::is_valid_email(&req.email) {
        tracing::error!(email=%req.email, "invalid email");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "invalid email".into(),
            }),
        )
            .into_response());
    }

    // All emails in fusionauth are stored lowercase
    let lowercase_email = req.email.to_lowercase();

    let (access_token, refresh_token) = match ctx
        .auth_client
        .password_login(&lowercase_email, &req.password)
        .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::trace!(error=?e, "unable to login user");
            match e {
                FusionAuthClientError::UserNotRegistered => {
                    ctx.auth_client
                        .register_user_from_email(&lowercase_email)
                        .await
                        .map_err(|e| {
                            tracing::trace!(error=?e, "unable to register user");
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    message: "unable to register user".into(),
                                }),
                            )
                                .into_response()
                        })?;

                    // User is now registered, re-login
                    ctx.auth_client
                        .password_login(&lowercase_email, &req.password)
                        .await
                        .map_err(|e| {
                            tracing::trace!(error=?e, "unable to login user");
                            password_login_error(e)
                        })?
                }
                error => return Err(password_login_error(error)),
            }
        }
    };

    cookies.add(create_access_token_cookie(&access_token));
    cookies.add(create_refresh_token_cookie(&refresh_token));

    Ok((
        StatusCode::OK,
        Json(UserTokensResponse {
            access_token,
            refresh_token,
        }),
    )
        .into_response())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn authentication_denials_are_not_reported_as_server_failures() {
        for (error, expected) in [
            (
                FusionAuthClientError::IncorrectCredentials,
                StatusCode::UNAUTHORIZED,
            ),
            (
                FusionAuthClientError::UserNotVerified,
                StatusCode::UNAUTHORIZED,
            ),
            (
                FusionAuthClientError::UserRegistrationNotVerified,
                StatusCode::UNAUTHORIZED,
            ),
            (
                FusionAuthClientError::PasswordChangeRequired,
                StatusCode::UNAUTHORIZED,
            ),
            (
                FusionAuthClientError::MultiFactorAuthenticationRequired,
                StatusCode::UNAUTHORIZED,
            ),
            (
                FusionAuthClientError::Generic(fusionauth::error::GenericErrorResponse {
                    message: "provider unavailable".into(),
                }),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ] {
            assert_eq!(password_login_error(error).status(), expected);
        }
    }
}
