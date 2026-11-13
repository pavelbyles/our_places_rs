//! Typed HTTP client for the backend APIs, shared by the portals.
//!
//! One module per area; everything is re-exported here so callers keep using
//! `common::app_client::<fn>`.

use crate::http_client::AuthenticatedClient;
use std::env;
use std::sync::OnceLock;

static CLIENT: OnceLock<AuthenticatedClient> = OnceLock::new();

/// Returns a global instance of the AuthenticatedClient
pub fn get_client() -> &'static AuthenticatedClient {
    CLIENT.get_or_init(|| {
        // Check if running in Cloud Run (var present) or generic prod env
        let is_cloud = env::var("EA__DATABASE__CLOUD").is_ok() || env::var("K_SERVICE").is_ok();
        AuthenticatedClient::new(is_cloud)
    })
}

/// Env var holding the active-currency cache TTL, in seconds.
static CURRENCY_CACHE_TTL_ENV: &str = "CURRENCY_CACHE_TTL_SECONDS";

/// Default active-currency cache TTL.
pub const DEFAULT_CURRENCY_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(60);

/// TTL of the portals' active-currency cache (`CURRENCY_CACHE_TTL_SECONDS`,
/// default 60, `0` disables caching). An unparseable value logs a warning and
/// uses the default: this caches display data, never money.
pub fn currency_cache_ttl() -> std::time::Duration {
    match env::var(CURRENCY_CACHE_TTL_ENV) {
        Err(_) => DEFAULT_CURRENCY_CACHE_TTL,
        Ok(raw) => raw
            .trim()
            .parse::<u64>()
            .map(std::time::Duration::from_secs)
            .unwrap_or_else(|e| {
                tracing::warn!(value = %raw, error = %e, "invalid CURRENCY_CACHE_TTL_SECONDS; using 60");
                DEFAULT_CURRENCY_CACHE_TTL
            }),
    }
}

// Env vars for API URLs
/// Base URL of the listing API.
pub fn listing_api_url() -> String {
    env::var("LISTING_API_URL")
        .unwrap_or_else(|_| "http://localhost:8082".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Base URL of the booking API.
pub fn booking_api_url() -> String {
    env::var("BOOKING_API_URL")
        .unwrap_or_else(|_| "http://localhost:8081".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Base URL of the user API.
pub fn user_api_url() -> String {
    env::var("USER_API_URL")
        .unwrap_or_else(|_| "http://localhost:8083".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Audience for tokens sent to the listing API. Defaults to its URL.
pub fn listing_api_audience() -> String {
    env::var("LISTING_API_AUDIENCE")
        .unwrap_or_else(|_| listing_api_url())
        .trim_end_matches('/')
        .to_string()
}

/// Audience for tokens sent to the booking API. Defaults to its URL.
pub fn booking_api_audience() -> String {
    env::var("BOOKING_API_AUDIENCE")
        .unwrap_or_else(|_| booking_api_url())
        .trim_end_matches('/')
        .to_string()
}

/// Audience for tokens sent to the user API. Defaults to its URL.
pub fn user_api_audience() -> String {
    env::var("USER_API_AUDIENCE")
        .unwrap_or_else(|_| user_api_url())
        .trim_end_matches('/')
        .to_string()
}

/// Enquiries, requests to book, conversations and the inbox.
pub mod booking_requests;
mod bookings;
mod listings;
mod payouts;
mod promo;
mod reservation_changes;
mod reviews;
mod users;
pub use bookings::{
    create_booking, delete_booking, get_all_bookings, get_booking_by_id,
    get_booking_document_by_number, get_booking_messages, get_cancellation_quote,
    get_listing_bookings, get_user_bookings, list_booking_documents, mark_booking_messages_read,
    send_booking_message, transfer_booking, update_booking, update_booking_with_auth,
};
pub use listings::{
    ListingSearchParams, ListingSearchRefusal, create_listing, create_price_override,
    delete_listing, delete_listing_image, delete_price_override, get_listing_by_id,
    get_price_overrides, rename_listing_image, search_listings, search_listings_at, update_listing,
};
pub use payouts::{get_host_payout_ledger, get_host_payout_summary, update_admin_payout_status};
pub use promo::{get_public_stay_quote, get_stay_quote, promo_refusal_in_error};
pub use reservation_changes::{
    approve_reservation_change_request, cancel_reservation_change_request,
    create_reservation_change_request, decline_reservation_change_request,
    list_reservation_change_requests, quote_reservation_change,
};
pub use reviews::{
    get_booking_review_token, get_listing_reviews, get_review_token_info, submit_host_reply,
    submit_review,
};
pub use users::{
    create_session, create_user, delete_session, fetch_user_profile, get_all_users,
    get_all_users_filtered, get_session, hard_delete_user, login_user, resend_verification,
    restore_user, revoke_user_sessions, soft_delete_user, update_user, verify_user,
};
