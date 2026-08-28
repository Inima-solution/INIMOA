use std::borrow::Cow;

use crate::{
    AuthedClient, FusionAuthClient, Result,
    error::{FusionAuthClientError, GenericErrorResponse},
};

#[cfg(test)]
mod test;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasswordLoginRequest<'a> {
    /// The application id
    pub application_id: Cow<'a, str>,
    /// The email or username of the user
    pub login_id: Cow<'a, str>,
    /// The password of the user
    pub password: Cow<'a, str>,
    /// Suppress access and refresh token generation for step-up verification.
    #[serde(rename = "noJWT")]
    pub no_jwt: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PasswordLoginResponse<'a> {
    /// The access token
    pub token: Cow<'a, str>,
    /// The refresh token
    pub refresh_token: Cow<'a, str>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MultiFactorLoginRequest<'a> {
    /// The application id.
    pub application_id: Cow<'a, str>,
    /// The short-lived MFA challenge identifier.
    pub two_factor_id: Cow<'a, str>,
    /// The proof supplied for the selected MFA method.
    pub code: Cow<'a, str>,
    /// Suppress access and refresh token generation for step-up verification.
    #[serde(rename = "noJWT")]
    pub no_jwt: bool,
    /// Never establish a trusted-device relationship while reauthenticating.
    pub trust_computer: bool,
}

#[derive(serde::Deserialize)]
struct MultiFactorLoginResponse<'a> {
    pub user: MultiFactorAuthenticatedUser<'a>,
}

#[derive(serde::Deserialize)]
struct MultiFactorAuthenticatedUser<'a> {
    pub email: Cow<'a, str>,
}

/// One available multi-factor method for completing reauthentication.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MultiFactorMethod {
    /// FusionAuth method identifier.
    pub id: String,
    /// Method kind, such as `authenticator`, `email`, or `sms`.
    pub method: String,
}

/// FusionAuth challenge returned when password verification also requires MFA.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MultiFactorChallenge {
    /// Short-lived identifier used to complete MFA.
    pub two_factor_id: String,
    /// Available MFA methods.
    pub methods: Vec<MultiFactorMethod>,
}

/// Result of checking the current user's password without creating a new session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordVerification {
    /// FusionAuth accepted the password and no MFA challenge is required.
    Verified,
    /// FusionAuth accepted the password but requires MFA completion.
    MultiFactorRequired(MultiFactorChallenge),
}

async fn send(
    client: &AuthedClient,
    base_url: &str,
    request: PasswordLoginRequest<'_>,
) -> Result<reqwest::Response> {
    client
        .client()
        .post(format!("{}/api/login", base_url))
        .json(&request)
        .send()
        .await
        .map_err(|error| {
            FusionAuthClientError::Generic(GenericErrorResponse {
                message: error.to_string(),
            })
        })
}

/// Performs a password login
/// https://fusionauth.io/docs/apis/login
/// Valid respones: 200, 202, 203, 212, 213, 242, 400, 401, 404, 409, 410, 423, 500, 503, 504
async fn login(
    client: &AuthedClient,
    base_url: &str,
    request: PasswordLoginRequest<'_>,
) -> Result<(String, String)> {
    let res = send(client, base_url, request).await?;

    let status = res.status();
    match status {
        reqwest::StatusCode::OK => {
            tracing::trace!("passwordless login complete");
            let body = res.json::<PasswordLoginResponse>().await.map_err(|e| {
                FusionAuthClientError::Generic(GenericErrorResponse {
                    message: e.to_string(),
                })
            })?;

            Ok((body.token.into(), body.refresh_token.into()))
        }
        reqwest::StatusCode::ACCEPTED => {
            tracing::warn!("user not registered to application");
            Err(FusionAuthClientError::UserNotRegistered)
        }
        status if status.as_u16() == 203 => Err(FusionAuthClientError::PasswordChangeRequired),
        status if status.as_u16() == 212 => Err(FusionAuthClientError::UserNotVerified),
        status if status.as_u16() == 213 => Err(FusionAuthClientError::UserRegistrationNotVerified),
        status if status.as_u16() == 242 => {
            Err(FusionAuthClientError::MultiFactorAuthenticationRequired)
        }
        reqwest::StatusCode::NOT_FOUND => Err(FusionAuthClientError::IncorrectCredentials),
        reqwest::StatusCode::CONFLICT | reqwest::StatusCode::GONE | reqwest::StatusCode::LOCKED => {
            Err(FusionAuthClientError::LoginPrevented)
        }
        _ => {
            tracing::error!(status=%status, "unexpected password login response from fusionauth");

            Err(FusionAuthClientError::Generic(GenericErrorResponse {
                message: format!("fusionauth returned {status}"),
            }))
        }
    }
}

async fn verify(
    client: &AuthedClient,
    base_url: &str,
    request: PasswordLoginRequest<'_>,
) -> Result<PasswordVerification> {
    let res = send(client, base_url, request).await?;
    let status = res.status();

    match status.as_u16() {
        200 => Ok(PasswordVerification::Verified),
        242 => res
            .json::<MultiFactorChallenge>()
            .await
            .map(PasswordVerification::MultiFactorRequired)
            .map_err(|error| {
                FusionAuthClientError::Generic(GenericErrorResponse {
                    message: error.to_string(),
                })
            }),
        202 => Err(FusionAuthClientError::UserNotRegistered),
        203 => Err(FusionAuthClientError::PasswordChangeRequired),
        212 => Err(FusionAuthClientError::UserNotVerified),
        213 => Err(FusionAuthClientError::UserRegistrationNotVerified),
        404 => Err(FusionAuthClientError::IncorrectCredentials),
        409 | 410 | 423 => Err(FusionAuthClientError::LoginPrevented),
        _ => {
            tracing::error!(status=%status, "unexpected password verification response from fusionauth");
            Err(FusionAuthClientError::Generic(GenericErrorResponse {
                message: format!("fusionauth returned {status}"),
            }))
        }
    }
}

/// Completes a FusionAuth MFA challenge without generating a session or tokens.
async fn verify_multi_factor(
    client: &AuthedClient,
    base_url: &str,
    request: MultiFactorLoginRequest<'_>,
) -> Result<String> {
    let res = client
        .client()
        .post(format!("{}/api/two-factor/login", base_url))
        .json(&request)
        .send()
        .await
        .map_err(|error| {
            FusionAuthClientError::Generic(GenericErrorResponse {
                message: error.to_string(),
            })
        })?;
    let status = res.status();

    match status.as_u16() {
        200 => res
            .json::<MultiFactorLoginResponse>()
            .await
            .map(|response| response.user.email.into_owned())
            .map_err(|error| {
                FusionAuthClientError::Generic(GenericErrorResponse {
                    message: error.to_string(),
                })
            }),
        202 | 203 | 212 | 213 | 404 | 409 | 410 | 421 => Err(FusionAuthClientError::IncorrectCode),
        _ => {
            tracing::error!(status=%status, "unexpected MFA verification response from fusionauth");
            Err(FusionAuthClientError::Generic(GenericErrorResponse {
                message: format!("fusionauth returned {status}"),
            }))
        }
    }
}

impl FusionAuthClient {
    /// Performs a password-based login with the given email and password.
    #[tracing::instrument(skip(self, password), fields(application_id=%self.client_id, fusion_auth_base_url=%self.fusion_auth_base_url))]
    pub async fn password_login(&self, email: &str, password: &str) -> Result<(String, String)> {
        login(
            &self.auth_client,
            &self.fusion_auth_base_url,
            PasswordLoginRequest {
                application_id: Cow::Borrowed(&self.client_id),
                login_id: Cow::Borrowed(email),
                password: Cow::Borrowed(password),
                no_jwt: false,
            },
        )
        .await
    }

    /// Verifies the current user's password without minting access or refresh tokens.
    #[tracing::instrument(skip(self, password), fields(application_id=%self.client_id, fusion_auth_base_url=%self.fusion_auth_base_url))]
    pub async fn verify_password(
        &self,
        email: &str,
        password: &str,
    ) -> Result<PasswordVerification> {
        verify(
            &self.auth_client,
            &self.fusion_auth_base_url,
            PasswordLoginRequest {
                application_id: Cow::Borrowed(&self.client_id),
                login_id: Cow::Borrowed(email),
                password: Cow::Borrowed(password),
                no_jwt: true,
            },
        )
        .await
    }

    /// Completes an existing MFA challenge and returns only the authenticated user's email.
    #[tracing::instrument(skip(self, code), fields(application_id=%self.client_id, fusion_auth_base_url=%self.fusion_auth_base_url))]
    pub async fn verify_multi_factor(&self, two_factor_id: &str, code: &str) -> Result<String> {
        verify_multi_factor(
            &self.auth_client,
            &self.fusion_auth_base_url,
            MultiFactorLoginRequest {
                application_id: Cow::Borrowed(&self.client_id),
                two_factor_id: Cow::Borrowed(two_factor_id),
                code: Cow::Borrowed(code),
                no_jwt: true,
                trust_computer: false,
            },
        )
        .await
    }
}
