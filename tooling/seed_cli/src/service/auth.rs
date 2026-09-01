//! Auth service wrapper.

use std::net::{IpAddr, Ipv4Addr};

#[cfg(test)]
pub use MockSeedAuth as Auth;
#[cfg(not(test))]
pub use SeedAuth as Auth;

use fusionauth::{FusionAuthClient, error::FusionAuthClientError};
#[allow(unused_imports)]
use mockall::automock;

#[cfg(test)]
mod test;

fn missing_user_is_none<T>(result: fusionauth::Result<T>) -> anyhow::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(FusionAuthClientError::UserDoesNotExist) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Wrapper around the FusionAuth client.
#[cfg_attr(test, allow(dead_code))]
pub struct SeedAuth {
    /// Fusionauth client
    inner: FusionAuthClient,
}

#[cfg_attr(test, automock)]
#[cfg_attr(test, allow(dead_code))]
impl SeedAuth {
    /// Create a new auth wrapper.
    pub fn new(inner: FusionAuthClient) -> Self {
        Self { inner }
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn create_user<'a>(
        &self,
        user: fusionauth::user::create::User<'a>,
    ) -> anyhow::Result<String> {
        let result = self
            .inner
            .create_user(
                user,
                true, /*skip_verification*/
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await?;

        Ok(result)
    }

    /// Whether a FusionAuth account exists for `email`. Read-only; also
    /// returns false when FusionAuth is unreachable.
    pub async fn user_exists(&self, email: &str) -> anyhow::Result<bool> {
        Ok(self.inner.get_user_id_by_email(email).await.is_ok())
    }

    /// Hard-delete a FusionAuth account by email. A missing account is already reset.
    #[tracing::instrument(skip(self, email), err)]
    pub async fn delete_user_by_email(&self, email: &str) -> anyhow::Result<()> {
        let Some(user_id) = missing_user_is_none(self.inner.get_user_id_by_email(email).await)?
        else {
            return Ok(());
        };
        let _ = missing_user_is_none(self.inner.delete_user(&user_id).await)?;
        Ok(())
    }

    /// Ensure a FusionAuth user exists and is registered to the application,
    /// so the account can log in through the real passwordless flow.
    /// Returns whether the user was created (false = already existed).
    ///
    /// Creating the user fires the `user.create` webhook, which writes the
    /// base macrodb rows — call this BEFORE seeding user rows directly.
    #[tracing::instrument(skip(self), err)]
    pub async fn ensure_user(&self, email: String) -> anyhow::Result<bool> {
        if self.inner.get_user_id_by_email(&email).await.is_ok() {
            return Ok(false);
        }

        self.create_user(fusionauth::user::create::User {
            email: std::borrow::Cow::Owned(email.clone()),
            username: None,
            password: "hardcodeLocalPassword123!".into(),
        })
        .await?;

        self.inner
            .register_user_from_email(&email)
            .await
            .inspect_err(|e| tracing::warn!(error=?e, "user registration failed, continuing"))
            .ok();

        Ok(true)
    }
}
