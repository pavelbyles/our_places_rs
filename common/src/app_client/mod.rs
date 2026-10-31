//! Typed HTTP client for the backend APIs, shared by the portals.
//!
//! One module per area; everything is re-exported here so callers keep using
//! `common::app_client::<fn>`.

use crate::http_client::AuthenticatedClient;
use crate::models::{
    BookingMessageResponse, BookingMessagesWrapper, BookingResponse, BookingReviewEligibility,
    CreateBookingMessageRequest, CreateSessionRequest, DynamicPricingQuote, HostReplyRequest,
    ListingDetails, ListingResponse, LoginRequest, MarkMessagesReadResponse, NewBookingRequest,
    NewReviewRequest, NewUserRequest, PriceOverride, ResendVerificationRequest, ReviewResponse,
    ReviewTokenInfoResponse, SessionResponse, TransferBookingRequest, UpdateUserRequest,
    UpdatedBookingRequest, UserResponse, VerifyRequest,
};
use crate::tax_document::TaxDocumentResponse;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use reqwest::Response;
use rust_decimal::Decimal;
use std::env;
use std::sync::OnceLock;
use uuid::Uuid;

static CLIENT: OnceLock<AuthenticatedClient> = OnceLock::new();

/// Returns a global instance of the AuthenticatedClient
pub fn get_client() -> &'static AuthenticatedClient {
    CLIENT.get_or_init(|| {
        // Check if running in Cloud Run (var present) or generic prod env
        let is_cloud = env::var("EA__DATABASE__CLOUD").is_ok() || env::var("K_SERVICE").is_ok();
        AuthenticatedClient::new(is_cloud)
    })
}

// Env vars for API URLs
pub fn listing_api_url() -> String {
    env::var("LISTING_API_URL")
        .unwrap_or_else(|_| "http://localhost:8082".to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn booking_api_url() -> String {
    env::var("BOOKING_API_URL")
        .unwrap_or_else(|_| "http://localhost:8081".to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn user_api_url() -> String {
    env::var("USER_API_URL")
        .unwrap_or_else(|_| "http://localhost:8083".to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn listing_api_audience() -> String {
    env::var("LISTING_API_AUDIENCE")
        .unwrap_or_else(|_| listing_api_url())
        .trim_end_matches('/')
        .to_string()
}

pub fn booking_api_audience() -> String {
    env::var("BOOKING_API_AUDIENCE")
        .unwrap_or_else(|_| booking_api_url())
        .trim_end_matches('/')
        .to_string()
}

pub fn user_api_audience() -> String {
    env::var("USER_API_AUDIENCE")
        .unwrap_or_else(|_| user_api_url())
        .trim_end_matches('/')
        .to_string()
}

mod bookings;
mod listings;
mod payouts;
mod reservation_changes;
mod reviews;
mod users;
pub use bookings::*;
pub use listings::*;
pub use payouts::*;
pub use reservation_changes::*;
pub use reviews::*;
pub use users::*;
