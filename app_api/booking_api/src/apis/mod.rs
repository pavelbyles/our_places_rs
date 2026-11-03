//! HTTP handlers for booking_api, grouped by resource.
//!
//! Shared helpers (portal URLs, pricing errors, confirmation codes, display
//! currency, email enqueueing, booking authorization) live here; each
//! submodule owns one route group and `routes` wires them together.

use actix_web::{FromRequest, HttpRequest};
use api_core::{error::ApiError, models::BookingResponse};
use chrono::Utc;
use db_core::booking_message as db_booking_message;
use rand::RngExt;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

mod availability;
mod booking_policy;
mod booking_updates;
mod bookings;
mod cron;
mod currencies;
mod documents;
mod exchange_rates;
mod ledger;
mod messages;
mod notifications;
mod promo_codes;
mod reservation_changes;
mod routes;
mod stay_pricing;

pub use availability::{AvailabilityParams, AvailabilityResponse, UnavailableRangesParams};
pub use booking_updates::UpdatedBookingRequest;
pub use cron::{
    CompleteStaysResponse, CronSweepResponse, CronSweepsResponse, complete_stays_cron,
    process_scheduled_notifications, run_sweeps, stay_completion_cutoff,
};
pub use routes::configure_routes;

static BASE_URL_ENV: &str = "BASE_URL";

static ADMIN_BASE_URL_ENV: &str = "ADMIN_BASE_URL";

/// Public URL of the guest portal, for links in emails (`BASE_URL`).
fn guest_base_url() -> String {
    std::env::var(BASE_URL_ENV)
        .unwrap_or_else(|_| "http://localhost:3010".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Public URL of the admin portal, for host links in emails
/// (`ADMIN_BASE_URL`).
fn admin_base_url() -> String {
    std::env::var(ADMIN_BASE_URL_ENV)
        .unwrap_or_else(|_| "http://localhost:3012".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn map_pricing_error(err: common::pricing::PricingError) -> ApiError {
    match err {
        common::pricing::PricingError::InvalidDateRange => {
            ApiError::Database(db_core::error::DbError::ValidationError(
                "Check-out date must be after check-in date".to_string(),
            ))
        }
        common::pricing::PricingError::MinNightsNotMet { required, provided } => {
            ApiError::Database(db_core::error::DbError::ValidationError(format!(
                "Minimum night stay requirement not met for seasonal period: required {}, provided {}",
                required, provided
            )))
        }
    }
}

/// The listing's base nightly price. A listing without one cannot be priced,
/// so this is a 400 rather than a silent zero.
fn nightly_price(listing: &db_core::models::Listing) -> Result<Decimal, ApiError> {
    listing.price_per_night.ok_or_else(|| {
        tracing::warn!(listing_id = %listing.id, "listing has no nightly price");
        ApiError::Database(db_core::error::DbError::ValidationError(
            "This listing has no nightly price and cannot be booked".to_string(),
        ))
    })
}

/// Whether `err` is a unique violation on `booking.confirmation_code`.
fn is_confirmation_code_collision(err: &db_core::error::DbError) -> bool {
    matches!(err, db_core::error::DbError::Sqlx(e)
    if e.as_database_error().is_some_and(|db| {
        db.is_unique_violation() && db.constraint() == Some("booking_confirmation_code_key")
    }))
}

pub fn generate_confirmation_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

    const LENGTH: usize = 8;
    let mut rng = rand::rng();

    (0..LENGTH)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Rate for showing a booking's amounts in `target`. A missing pair is a 422
/// (`FX_RATE_UNAVAILABLE`) so amounts are never shown under the wrong label.
async fn display_rate(pool: &PgPool, from: &str, target: &str) -> Result<Decimal, ApiError> {
    db_core::exchange_rate::get_rate_pool(
        pool,
        from,
        target,
        Utc::now(),
        db_core::exchange_rate::RateUse::Settlement,
    )
    .await
    .map_err(|e| {
        tracing::warn!(from, to = target, error = %e, "booking display currency conversion failed");
        ApiError::Database(e)
    })
}

/// Restates a booking's money fields in `target` at `rate`.
fn convert_booking_amounts(booking: &mut BookingResponse, rate: Decimal, target: &str) {
    booking.daily_rate = (booking.daily_rate * rate).round_dp(2);
    booking.sub_total_price = (booking.sub_total_price * rate).round_dp(2);
    booking.total_price = (booking.total_price * rate).round_dp(2);
    booking.discount_value = booking.discount_value.map(|d| (d * rate).round_dp(2));
    booking.tax_value = booking.tax_value.map(|t| (t * rate).round_dp(2));
    booking.cleaning_fee = booking.cleaning_fee.map(|f| (f * rate).round_dp(2));
    booking.damage_deposit = booking.damage_deposit.map(|f| (f * rate).round_dp(2));
    booking.platform_fee = booking.platform_fee.map(|f| (f * rate).round_dp(2));
    booking.promo_discount = (booking.promo_discount * rate).round_dp(2);
    booking.currency = target.trim().to_ascii_uppercase();
}

/// Queues a transactional email and hands it to the email worker.
///
/// Best-effort: a failure never fails the caller, but is logged with the
/// template and subject (and outbox id once queued) so it can be traced or
/// replayed. Returns whether the email was queued.
async fn enqueue_email(
    pool: &PgPool,
    recipient: &str,
    subject: &str,
    template: &str,
    payload: &impl Serialize,
) -> bool {
    let payload = match serde_json::to_value(payload) {
        Ok(payload) => payload,
        Err(e) => {
            tracing::error!(template, subject, error = %e, "email not queued: payload failed to serialize");
            return false;
        }
    };
    let outbox = match db_core::email_outbox::insert_email_outbox(
        pool, recipient, subject, template, &payload, 3,
    )
    .await
    {
        Ok(outbox) => outbox,
        Err(e) => {
            tracing::error!(template, subject, error = ?e, "email not queued: outbox insert failed");
            return false;
        }
    };
    let email_id = outbox.id;
    let template = template.to_string();
    tokio::spawn(async move {
        let publisher = api_core::email_publisher::EmailPublisher::from_env();
        if let Err(e) = publisher.publish_email_event(email_id).await {
            tracing::error!(%email_id, %template, error = ?e, "email queued but not published; it will not be sent until the event is republished");
        }
    });
    true
}

#[derive(Deserialize, Debug)]
pub struct CurrencyQuery {
    pub currency: Option<String>,
}

/// Helper function enforcing that only booking guest, listing host, or administrator can view tax documents.
/// Non-authenticated callers or unauthorized users receive 404 (do not leak existence).
async fn check_booking_authorization(
    req: &HttpRequest,
    pool: &PgPool,
    booking_id: Uuid,
) -> Result<(), ApiError> {
    let _ = authorize_booking_party(req, pool, booking_id).await?;
    Ok(())
}

/// A 404 for anything the caller may not see, so IDs cannot be probed.
fn not_found() -> ApiError {
    ApiError::Database(db_core::error::DbError::Sqlx(sqlx::Error::RowNotFound))
}

fn is_admin(user: &db_core::models::User) -> bool {
    user.roles.contains(&db_core::models::UserRole::Admin)
}

/// The user behind `claims`. A token for a user that no longer exists, or
/// whose account has been deactivated or deleted, is a 401: the account's
/// tokens stop working as soon as it is switched off, not when they expire.
/// A database failure is passed up (and logged), never hidden as a 404.
async fn caller(
    pool: &PgPool,
    claims: &api_core::auth::Claims,
) -> Result<db_core::models::User, ApiError> {
    db_core::user::get_user_by_id(pool, claims.sub)
        .await
        .map_err(|e| match e {
            db_core::error::DbError::Sqlx(sqlx::Error::RowNotFound) => {
                tracing::warn!(user_id = %claims.sub, "token is for a user that does not exist");
                ApiError::Unauthorized("Unknown user".to_string())
            }
            e => {
                tracing::error!(user_id = %claims.sub, error = ?e, "failed to load the calling user");
                ApiError::Database(e)
            }
        })
        .and_then(|user| {
            if user.is_active && user.deleted_at.is_none() {
                Ok(user)
            } else {
                tracing::warn!(
                    user_id = %claims.sub,
                    is_active = user.is_active,
                    deleted = user.deleted_at.is_some(),
                    "token is for a deactivated or deleted account"
                );
                Err(ApiError::Unauthorized("Account is not active".to_string()))
            }
        })
}

/// The calling user, who must be an administrator (403 otherwise).
async fn require_admin(
    pool: &PgPool,
    claims: &api_core::auth::Claims,
    action: &str,
) -> Result<db_core::models::User, ApiError> {
    let user = caller(pool, claims).await?;
    if is_admin(&user) {
        Ok(user)
    } else {
        tracing::warn!(user_id = %claims.sub, action, "non-admin attempted an admin-only action");
        Err(ApiError::Forbidden(format!(
            "Only administrators can {action}"
        )))
    }
}

/// The calling user, who must be an administrator or a host (403 otherwise).
async fn require_staff(
    pool: &PgPool,
    claims: &api_core::auth::Claims,
    action: &str,
) -> Result<db_core::models::User, ApiError> {
    let user = caller(pool, claims).await?;
    let staff = user.roles.iter().any(|r| {
        matches!(
            r,
            db_core::models::UserRole::Admin | db_core::models::UserRole::Host
        )
    });
    if staff {
        Ok(user)
    } else {
        tracing::warn!(user_id = %claims.sub, action, "non-staff attempted a staff-only action");
        Err(ApiError::Forbidden(format!(
            "Only administrators and hosts can {action}"
        )))
    }
}

/// Validates that the request carries a valid JWT for the booking guest, listing host, or an administrator.
/// Returns `404 Not Found` for unauthenticated callers or strangers to prevent IDOR enumeration.
async fn authorize_booking_party(
    req: &HttpRequest,
    pool: &PgPool,
    booking_id: Uuid,
) -> Result<(db_core::models::BookingParties, db_core::models::User), ApiError> {
    let claims = api_core::auth::Claims::from_request(req, &mut actix_web::dev::Payload::None)
        .into_inner()
        .map_err(|_| not_found())?;

    let parties = db_booking_message::get_booking_parties(pool, booking_id)
        .await
        .map_err(|e| {
            tracing::error!(%booking_id, error = ?e, "failed to load booking parties");
            ApiError::Database(e)
        })?
        .ok_or_else(not_found)?;

    let user = caller(pool, &claims).await?;

    if !is_admin(&user) && claims.sub != parties.guest_id && claims.sub != parties.host_id {
        tracing::warn!(%booking_id, user_id = %claims.sub, "booking access denied: caller is not a party");
        return Err(not_found());
    }

    Ok((parties, user))
}

#[cfg(test)]
mod authz_tests;
#[cfg(test)]
mod past_date_tests;
#[cfg(test)]
mod promo_hold_tests;
#[cfg(test)]
mod rate_tests;
#[cfg(test)]
mod stay_pricing_pin_tests;
#[cfg(test)]
mod tests;
