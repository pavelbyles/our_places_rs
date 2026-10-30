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
        // Behind Cloud Run IAM or API Gateway, `Authorization` holds a Google
        // ID token and the user's JWT arrives in `X-Forwarded-Authorization`.
        let headers = req.headers();
        let claims = headers
            .get(common::http_client::USER_AUTHORIZATION_HEADER)
            .or_else(|| headers.get("Authorization"))
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
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    fn claims_of(req: &HttpRequest) -> Result<Claims, ApiError> {
        Claims::from_request(req, &mut Payload::None).into_inner()
    }

    #[test]
    fn reads_the_user_token_from_authorization_when_called_directly() {
        let user = Uuid::now_v7();
        let req = TestRequest::default()
            .insert_header((
                "Authorization",
                format!("Bearer {}", generate_test_jwt(user)),
            ))
            .to_http_request();
        assert_eq!(claims_of(&req).map(|c| c.sub).ok(), Some(user));
    }

    #[test]
    fn prefers_the_forwarded_user_token_over_the_service_identity_token() {
        let user = Uuid::now_v7();
        let req = TestRequest::default()
            .insert_header(("Authorization", "Bearer google-id-token"))
            .insert_header((
                "X-Forwarded-Authorization",
                format!("Bearer {}", generate_test_jwt(user)),
            ))
            .to_http_request();
        assert_eq!(claims_of(&req).map(|c| c.sub).ok(), Some(user));
    }

    #[test]
    fn rejects_a_request_without_a_user_token() {
        let req = TestRequest::default()
            .insert_header(("Authorization", "Bearer google-id-token"))
            .to_http_request();
        assert!(claims_of(&req).is_err());
    }
}
