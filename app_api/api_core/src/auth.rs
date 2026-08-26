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
        // Extract Authorization header
        let auth_header = match req.headers().get("Authorization") {
            Some(header) => header,
            None => {
                return ready(Err(ApiError::Unauthorized(
                    "Missing Authorization header".into(),
                )));
            }
        };

        let auth_str = match auth_header.to_str() {
            Ok(s) => s,
            Err(_) => {
                return ready(Err(ApiError::Unauthorized(
                    "Invalid Authorization header format".into(),
                )));
            }
        };

        if !auth_str.starts_with("Bearer ") {
            return ready(Err(ApiError::Unauthorized(
                "Invalid Authorization header format".into(),
            )));
        }

        let token = &auth_str[7..];
        let secret = env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());

        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        match decode::<Claims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        ) {
            Ok(token_data) => ready(Ok(token_data.claims)),
            Err(_) => ready(Err(ApiError::Unauthorized(
                "Invalid or expired token".into(),
            ))),
        }
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
