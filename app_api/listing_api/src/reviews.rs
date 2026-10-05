use actix_web::middleware::from_fn;
use actix_web::{HttpRequest, Responder, web};
use api_core::api_common::content_negotiation_middleware;
use api_core::error::ApiError;
use api_core::response::{Payload, respond};
use common::models::{
    BookingReviewEligibility, HostReplyRequest, NewReviewRequest, ReviewResponse,
    ReviewTokenInfoResponse,
};
use db_core::review as db_review;
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

#[tracing::instrument]
#[utoipa::path(
    get,
    path = "/api/v1/reviews/token/{token}",
    tag = "reviews",
    params(
        ("token" = String, Path, description = "Review Token")
    ),
    responses(
        (status = 200, description = "Token info retrieved", body = ReviewTokenInfoResponse),
        (status = 404, description = "Token not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_review_token_info(
    pool: web::Data<PgPool>,
    req: HttpRequest,
    token: web::Path<String>,
) -> Result<impl Responder, ApiError> {
    let token_str = token.into_inner();
    let token_details = sqlx::query!(
        r#"
        SELECT 
            rt.valid_from, rt.expires_at, rt.used_at,
            l.name as listing_name,
            u.first_name as guest_first_name,
            b.date_from as check_in, b.date_to as check_out
        FROM review_token rt
        JOIN listing l ON rt.listing_id = l.id
        JOIN "user" u ON rt.guest_id = u.id
        JOIN booking b ON rt.booking_id = b.id
        WHERE rt.token = $1
        "#,
        token_str
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| ApiError::Database(db_core::error::DbError::Sqlx(e)))?
    .ok_or_else(|| ApiError::Database(db_core::error::DbError::Sqlx(sqlx::Error::RowNotFound)))?;

    let now = chrono::Utc::now();
    let is_valid = token_details.used_at.is_none()
        && token_details.valid_from <= now
        && token_details.expires_at >= now;

    let error_code = if token_details.used_at.is_some() {
        Some("ALREADY_USED".to_string())
    } else if token_details.valid_from > now {
        Some("NOT_YET_VALID".to_string())
    } else if token_details.expires_at < now {
        Some("EXPIRED".to_string())
    } else {
        None
    };

    // Create a response representation
    let response = ReviewTokenInfoResponse {
        is_valid,
        listing_name: token_details.listing_name,
        guest_first_name: token_details.guest_first_name,
        check_in: token_details.check_in,
        check_out: token_details.check_out,
        expires_at: token_details.expires_at,
        error_code,
    };

    Ok(respond(
        &req,
        Payload::Item(response),
        |_: Vec<ReviewTokenInfoResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip(payload))]
#[utoipa::path(
    post,
    path = "/api/v1/reviews/token/{token}",
    tag = "reviews",
    request_body = NewReviewRequest,
    params(
        ("token" = String, Path, description = "Review Token")
    ),
    responses(
        (status = 200, description = "Review submitted"),
        (status = 400, description = "Invalid token or review data"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn submit_review(
    pool: web::Data<PgPool>,
    req: HttpRequest,
    token: web::Path<String>,
    payload: web::Json<NewReviewRequest>,
) -> Result<impl Responder, ApiError> {
    let token_str = token.into_inner();
    let review_req = payload.into_inner();

    // Ensure token in path matches token in body if provided, or override
    if review_req.token != token_str {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Token in path does not match token in body".to_string(),
            ),
        ));
    }

    if let Err(e) = review_req.validate() {
        return Err(ApiError::ValidationError(e));
    }

    let review = db_review::create_review_with_token(pool.get_ref(), &token_str, &review_req)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(review),
        |_: Vec<db_core::models::Review>| (),
        actix_web::http::StatusCode::CREATED,
    ))
}

#[tracing::instrument(skip(payload))]
#[utoipa::path(
    post,
    path = "/api/v1/reviews/{id}/reply",
    tag = "reviews",
    request_body = HostReplyRequest,
    params(
        ("id" = String, Path, description = "Review UUID")
    ),
    responses(
        (status = 200, description = "Host reply submitted"),
        (status = 400, description = "Invalid reply or review already replied"),
        (status = 404, description = "Review not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn submit_host_reply(
    pool: web::Data<PgPool>,
    req: HttpRequest,
    claims: api_core::auth::Claims,
    review_id: web::Path<Uuid>,
    payload: web::Json<HostReplyRequest>,
) -> Result<impl Responder, ApiError> {
    let id = review_id.into_inner();
    let reply_req = payload.into_inner();

    if let Err(e) = reply_req.validate() {
        return Err(ApiError::ValidationError(e));
    }

    let host_id = claims.sub;

    let review = db_review::add_host_reply(pool.get_ref(), id, host_id, &reply_req.reply_text)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(review),
        |_: Vec<db_core::models::Review>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct ReviewsQueryParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

fn default_page() -> i64 {
    1
}
fn default_per_page() -> i64 {
    20
}

#[tracing::instrument]
#[utoipa::path(
    get,
    path = "/api/v1/listings/{id}/reviews",
    tag = "reviews",
    params(
        ("id" = String, Path, description = "Listing UUID"),
        ReviewsQueryParams
    ),
    responses(
        (status = 200, description = "Listing reviews retrieved", body = Vec<ReviewResponse>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_listing_reviews_handler(
    pool: web::Data<PgPool>,
    req: HttpRequest,
    listing_id: web::Path<Uuid>,
    query: web::Query<ReviewsQueryParams>,
) -> Result<impl Responder, ApiError> {
    let id = listing_id.into_inner();
    let offset = (query.page - 1) * query.per_page;

    let reviews = db_review::get_listing_reviews(pool.get_ref(), id, query.per_page, offset)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Collection(reviews),
        |_: Vec<ReviewResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    get,
    path = "/api/v1/reviews/booking/{booking_id}/token",
    tag = "reviews",
    params(
        ("booking_id" = Uuid, Path, description = "Booking UUID")
    ),
    responses(
        (status = 200, description = "Booking review eligibility and token", body = BookingReviewEligibility),
        (status = 404, description = "Booking not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_booking_review_token(
    pool: web::Data<PgPool>,
    req: HttpRequest,
    claims: api_core::auth::Claims,
    booking_id: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let b_id = booking_id.into_inner();

    let guest_id = Some(claims.sub);

    let target_guest_id = match guest_id {
        Some(uid) => uid,
        None => {
            let row = sqlx::query!("SELECT guest_id FROM booking WHERE id = $1", b_id)
                .fetch_optional(pool.get_ref())
                .await
                .map_err(|e| ApiError::Database(db_core::error::DbError::Sqlx(e)))?
                .ok_or_else(|| {
                    ApiError::Database(db_core::error::DbError::Sqlx(sqlx::Error::RowNotFound))
                })?;
            row.guest_id
        }
    };

    let eligibility =
        db_review::get_or_create_booking_review_token(pool.get_ref(), b_id, target_guest_id)
            .await
            .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(eligibility),
        |_: Vec<BookingReviewEligibility>| (),
        actix_web::http::StatusCode::OK,
    ))
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1/reviews")
            .route(
                "/token/{token}",
                web::get()
                    .to(get_review_token_info)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/token/{token}",
                web::post()
                    .to(submit_review)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/booking/{booking_id}/token",
                web::get()
                    .to(get_booking_review_token)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/reply",
                web::post()
                    .to(submit_host_reply)
                    .wrap(from_fn(content_negotiation_middleware)),
            ),
    );
}
