use fusionauth::error::FusionAuthClientError;

use super::missing_user_is_none;

#[test]
fn missing_user_is_idempotent() {
    assert!(
        missing_user_is_none::<()>(Err(FusionAuthClientError::UserDoesNotExist))
            .unwrap()
            .is_none()
    );
}

#[test]
fn existing_user_passes_through() {
    assert_eq!(
        missing_user_is_none(Ok("user".to_string())).unwrap(),
        Some("user".to_string())
    );
}

#[test]
fn non_missing_error_fails_closed() {
    assert!(missing_user_is_none::<()>(Err(FusionAuthClientError::UserNotVerified)).is_err());
}
