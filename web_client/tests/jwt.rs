//! Tests for service JWT minting.

use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;
use uuid::Uuid;
use web_client::jwt::{JwtError, SERVICE_TOKEN_TTL_SECS, ServiceJwt};

#[derive(Deserialize)]
struct Claims {
    sub: Uuid,
    exp: i64,
}

#[test]
fn empty_secret_is_rejected() {
    assert!(matches!(
        ServiceJwt::from_secret(""),
        Err(JwtError::MissingSecret)
    ));
    assert!(matches!(
        ServiceJwt::from_secret("   "),
        Err(JwtError::MissingSecret)
    ));
}

#[test]
fn minted_token_carries_subject_and_short_expiry() -> Result<(), Box<dyn std::error::Error>> {
    let user = Uuid::now_v7();
    let token = ServiceJwt::from_secret("test-secret")?.mint(user)?;
    let claims = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(b"test-secret"),
        &Validation::new(Algorithm::HS256),
    )?
    .claims;
    let now = Utc::now().timestamp();
    assert_eq!(claims.sub, user);
    assert!(claims.exp > now && claims.exp <= now + SERVICE_TOKEN_TTL_SECS);
    Ok(())
}

#[test]
fn debug_output_hides_the_secret() -> Result<(), JwtError> {
    let jwt = ServiceJwt::from_secret("super-secret-value")?;
    assert!(!format!("{jwt:?}").contains("super-secret-value"));
    Ok(())
}
