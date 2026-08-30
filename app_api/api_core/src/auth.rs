use crate::error::ApiError;
use actix_web::dev::Payload;
use actix_web::{FromRequest, HttpRequest};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use std::env;
use std::future::{Ready, ready};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: usize,
}

impl FromRequest for Claims {
    type Error = ApiError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let claims = req
            .headers()
            .get("Authorization")
            .ok_or_else(|| ApiError::Unauthorized("Missing Authorization header".into()))
            .and_then(|header| {
                header.to_str().map_err(|_| {
                    ApiError::Unauthorized("Invalid Authorization header format".into())
                })
            })
            .and_then(|auth_str| {
                auth_str.strip_prefix("Bearer ").ok_or_else(|| {
                    ApiError::Unauthorized("Invalid Authorization header format".into())
                })
            })
            .and_then(|token| {
                let secret = env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
                let mut validation = Validation::new(Algorithm::HS256);
                validation.validate_exp = true;

                decode::<Claims>(
                    token,
                    &DecodingKey::from_secret(secret.as_bytes()),
                    &validation,
                )
                .map(|token_data| token_data.claims)
                .map_err(|_| ApiError::Unauthorized("Invalid or expired token".into()))
            });

        ready(claims)
    }
}

pub fn generate_test_jwt(user_id: Uuid) -> String {
    use jsonwebtoken::{EncodingKey, Header, encode};

    let claims = Claims {
        sub: user_id,
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
    };

    let secret = env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}
