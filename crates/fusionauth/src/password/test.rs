use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path},
};

use super::*;

fn client(server: &MockServer) -> FusionAuthClient {
    FusionAuthClient::new(
        "api-key".into(),
        "application-id".into(),
        "client-secret".into(),
        server.uri(),
        "http://localhost/oauth/redirect".into(),
        "google-client-id".into(),
        "google-client-secret".into(),
    )
}

#[tokio::test]
async fn password_login_keeps_token_generation_and_maps_authentication_denials() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .and(body_json(json!({
            "applicationId": "application-id",
            "loginId": "actor@example.com",
            "password": "secret",
            "noJWT": false
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "access-token",
            "refreshToken": "refresh-token"
        })))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(
        client(&server)
            .password_login("actor@example.com", "secret")
            .await
            .unwrap(),
        ("access-token".into(), "refresh-token".into())
    );

    for (status, expected) in [
        (203, FusionAuthClientError::PasswordChangeRequired),
        (212, FusionAuthClientError::UserNotVerified),
        (213, FusionAuthClientError::UserRegistrationNotVerified),
        (
            242,
            FusionAuthClientError::MultiFactorAuthenticationRequired,
        ),
        (404, FusionAuthClientError::IncorrectCredentials),
        (409, FusionAuthClientError::LoginPrevented),
        (410, FusionAuthClientError::LoginPrevented),
        (423, FusionAuthClientError::LoginPrevented),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let error = client(&server)
            .password_login("actor@example.com", "secret")
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), expected.to_string());
    }
}

#[tokio::test]
async fn unexpected_provider_body_is_not_returned_in_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .respond_with(ResponseTemplate::new(500).set_body_string("sensitive-provider-body"))
        .mount(&server)
        .await;

    let error = client(&server)
        .verify_password("actor@example.com", "secret")
        .await
        .unwrap_err();
    let FusionAuthClientError::Generic(error) = error else {
        panic!("expected generic upstream error");
    };
    assert!(!error.message.contains("sensitive-provider-body"));
    assert!(error.message.contains("500"));
}

#[tokio::test]
async fn verify_password_suppresses_token_generation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .and(header("authorization", "api-key"))
        .and(body_json(json!({
            "applicationId": "application-id",
            "loginId": "actor@example.com",
            "password": "secret",
            "noJWT": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    let result = client(&server)
        .verify_password("actor@example.com", "secret")
        .await
        .unwrap();
    assert_eq!(result, PasswordVerification::Verified);
}

#[tokio::test]
async fn verify_password_returns_mfa_challenge_for_status_242() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .respond_with(ResponseTemplate::new(242).set_body_json(json!({
            "twoFactorId": "challenge-id",
            "methods": [{ "id": "TOTP", "method": "authenticator" }]
        })))
        .mount(&server)
        .await;

    let result = client(&server)
        .verify_password("actor@example.com", "secret")
        .await
        .unwrap();
    assert_eq!(
        result,
        PasswordVerification::MultiFactorRequired(MultiFactorChallenge {
            two_factor_id: "challenge-id".into(),
            methods: vec![MultiFactorMethod {
                id: "TOTP".into(),
                method: "authenticator".into(),
            }],
        })
    );
}

#[tokio::test]
async fn verify_password_distinguishes_registration_mfa_and_bad_credentials() {
    for (status, expected) in [
        (202, FusionAuthClientError::UserNotRegistered),
        (203, FusionAuthClientError::PasswordChangeRequired),
        (212, FusionAuthClientError::UserNotVerified),
        (213, FusionAuthClientError::UserRegistrationNotVerified),
        (404, FusionAuthClientError::IncorrectCredentials),
        (409, FusionAuthClientError::LoginPrevented),
        (410, FusionAuthClientError::LoginPrevented),
        (423, FusionAuthClientError::LoginPrevented),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;

        let error = client(&server)
            .verify_password("actor@example.com", "secret")
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), expected.to_string());
    }
}
