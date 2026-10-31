use std::env;

use chrono::Utc;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::Serialize;
use uuid::Uuid;

static JWT_SECRET_ENV: &str = "JWT_SECRET";

/// Lifetime of a minted service token, in seconds.
pub const SERVICE_TOKEN_TTL_SECS: i64 = 300;

/// Why a service token could not be created.
#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    /// `JWT_SECRET` is unset or blank. There is no fallback secret.
    #[error("JWT_SECRET is not set")]
    MissingSecret,
    /// Signing the token failed.
    #[error("failed to sign service token: {0}")]
    Encode(#[from] jsonwebtoken::errors::Error),
}

#[derive(Serialize)]
struct Claims {
    sub: Uuid,
    exp: i64,
}

/// Mints short-lived HS256 tokens that let the portal call the backend APIs
/// on behalf of a signed-in user.
#[derive(Clone)]
pub struct ServiceJwt {
    key: EncodingKey,
}

impl std::fmt::Debug for ServiceJwt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceJwt").finish_non_exhaustive()
    }
}

impl ServiceJwt {
    /// Reads `JWT_SECRET`; fails if it is unset or blank.
    pub fn from_env() -> Result<Self, JwtError> {
        env::var(JWT_SECRET_ENV)
            .map_err(|_| JwtError::MissingSecret)
            .and_then(|secret| Self::from_secret(&secret))
    }

    /// Builds a minter from an explicit secret; fails if it is blank.
    pub fn from_secret(secret: &str) -> Result<Self, JwtError> {
        Some(secret)
            .filter(|s| !s.trim().is_empty())
            .map(|s| Self {
                key: EncodingKey::from_secret(s.as_bytes()),
            })
            .ok_or(JwtError::MissingSecret)
    }

    /// Mints a token for `user_id` that expires in [`SERVICE_TOKEN_TTL_SECS`].
    pub fn mint(&self, user_id: Uuid) -> Result<String, JwtError> {
        let claims = Claims {
            sub: user_id,
            exp: Utc::now().timestamp() + SERVICE_TOKEN_TTL_SECS,
        };
        Ok(encode(&Header::default(), &claims, &self.key)?)
    }
}
