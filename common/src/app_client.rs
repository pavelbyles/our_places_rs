use crate::http_client::AuthenticatedClient;
use crate::models::{
    BookingResponse, BookingReviewEligibility, CreateSessionRequest, DynamicPricingQuote,
    HostReplyRequest, ListingDetails, ListingResponse, LoginRequest, NewBookingRequest,
    NewReviewRequest, PriceOverride, ReviewResponse, ReviewTokenInfoResponse, SessionResponse,
    TransferBookingRequest, UpdateUserRequest, UpdatedBookingRequest, UserResponse,
};

use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use reqwest::Response;
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

// -----------------------------------------------------------------------------
// Listing API Clients
// -----------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct ListingSearchParams {
    pub name: Option<String>,
    pub owner_email: Option<String>,
    pub listing_structure: Option<Vec<String>>,
    pub max_price: Option<f64>,
    pub currency: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

pub async fn search_listings(params: ListingSearchParams) -> Result<Vec<ListingResponse>> {
    let api_url = listing_api_url();
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(20);
    let mut url = format!(
        "{}/api/v1/listings?page={}&per_page={}",
        api_url, page, per_page
    );

    if let Some(s) = params.name.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&name={}", s));
    }
    if let Some(s) = params.owner_email.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&owner={}", s));
    }
    if let Some(structures) = params.listing_structure.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&structure_type={}", structures.join(",")));
    }
    if let Some(s) = params.max_price.filter(|&s| s > 0.0) {
        url.push_str(&format!("&max_price={}", s));
    }
    if let Some(c) = params.currency.filter(|c| !c.is_empty()) {
        url.push_str(&format!("&currency={}", c));
    }

    let audience = listing_api_audience();
    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to fetch listings: {}", res.status());
    }

    res.json::<Vec<ListingResponse>>()
        .await
        .context("Failed to deserialize listings response")
}

pub async fn get_listing_by_id(id: &str, currency: Option<&str>) -> Result<ListingDetails> {
    let api_url = listing_api_url();
    let mut url = format!("{}/api/v1/listings/{}", api_url, id);
    if let Some(c) = currency.filter(|c| !c.is_empty()) {
        url.push_str(&format!("?currency={}", c));
    }

    let audience = listing_api_audience();
    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to fetch listing details: {}", res.status());
    }

    res.json::<ListingDetails>()
        .await
        .context("Failed to deserialize listing details")
}

pub async fn create_listing(req: &crate::models::NewListingRequest) -> Result<ListingResponse> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings", api_url);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to create listing ({}): {}", status, err_text);
    }

    res.json::<ListingResponse>()
        .await
        .context("Failed to parse listing response")
}

pub async fn update_listing(
    id: &str,
    req: &crate::models::UpdatedListingRequest,
) -> Result<ListingResponse> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}", api_url, id);

    let res = get_client()
        .patch(&url, &audience, req)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to update listing ({}): {}", status, err_text);
    }

    res.json::<ListingResponse>()
        .await
        .context("Failed to parse listing response")
}

pub async fn delete_listing(id: &str) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}", api_url, id);

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete listing ({}): {}", status, err_text);
    }

    Ok(())
}

pub async fn get_price_overrides(listing_id: Uuid) -> Result<Vec<PriceOverride>> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}/price-overrides", api_url, listing_id);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        return Ok(Vec::new());
    }

    res.json::<Vec<PriceOverride>>()
        .await
        .context("Failed to deserialize price overrides")
}

pub async fn create_price_override(
    listing_id: Uuid,
    req: &crate::models::CreatePriceOverrideRequest,
) -> Result<PriceOverride> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}/price-overrides", api_url, listing_id);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to create price override ({}): {}", status, err_text);
    }

    res.json::<PriceOverride>()
        .await
        .context("Failed to parse price override response")
}

pub async fn delete_price_override(listing_id: Uuid, override_id: Uuid) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!(
        "{}/api/v1/listings/{}/price-overrides/{}",
        api_url, listing_id, override_id
    );

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete price override ({}): {}", status, err_text);
    }

    Ok(())
}

pub async fn get_pricing_quote(
    listing_id: Uuid,
    check_in: NaiveDate,
    check_out: NaiveDate,
    currency: Option<&str>,
) -> Result<DynamicPricingQuote> {
    use rust_decimal::Decimal;

    let listing_details = get_listing_by_id(&listing_id.to_string(), currency).await?;
    let base_nightly_rate = listing_details
        .listing
        .price_per_night
        .unwrap_or(Decimal::ZERO);

    let all_overrides = get_price_overrides(listing_id).await.unwrap_or_default();
    let active_overrides: Vec<PriceOverride> = all_overrides
        .into_iter()
        .filter(|ovr| ovr.start_date < check_out && ovr.end_date > check_in)
        .collect();

    crate::pricing::calculate_dynamic_quote(
        base_nightly_rate,
        listing_details.listing.minimum_stay,
        &active_overrides,
        check_in,
        check_out,
    )
    .map_err(|e| anyhow::anyhow!("Pricing calculation failed: {}", e))
}

// -----------------------------------------------------------------------------
// Booking API Clients
// -----------------------------------------------------------------------------

pub async fn create_booking(req: &NewBookingRequest) -> Result<BookingResponse> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings", api_url);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to create booking ({}): {}", status, err_text);
    }

    res.json::<BookingResponse>()
        .await
        .context("Failed to parse booking response")
}

pub async fn get_booking_by_id(id: Uuid, currency: Option<&str>) -> Result<BookingResponse> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let mut url = format!("{}/api/v1/bookings/{}", api_url, id);
    if let Some(c) = currency.filter(|c| !c.is_empty()) {
        url.push_str(&format!("?currency={}", c));
    }

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch booking details ({}): {}", status, err_text);
    }

    res.json::<BookingResponse>()
        .await
        .context("Failed to parse booking response")
}

pub async fn update_booking(id: Uuid, req: &UpdatedBookingRequest) -> Result<BookingResponse> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}", api_url, id);

    let res = get_client()
        .patch(&url, &audience, req)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to update booking ({}): {}", status, err_text);
    }

    res.json::<BookingResponse>()
        .await
        .context("Failed to parse booking response")
}

pub async fn delete_booking(id: Uuid) -> Result<()> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}", api_url, id);

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete booking ({}): {}", status, err_text);
    }

    Ok(())
}

pub async fn transfer_booking(id: Uuid, new_guest_id: Uuid) -> Result<BookingResponse> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}/transfer", api_url, id);

    let req = TransferBookingRequest {
        guest_id: new_guest_id,
    };

    let res = get_client()
        .post(&url, &audience, &req)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to transfer booking ({}): {}", status, err_text);
    }

    res.json::<BookingResponse>()
        .await
        .context("Failed to parse booking response")
}

pub async fn get_user_bookings(user_id: Uuid) -> Result<Vec<BookingResponse>> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/user/{}", api_url, user_id);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch user bookings ({}): {}", status, err_text);
    }

    res.json::<Vec<BookingResponse>>()
        .await
        .context("Failed to parse user bookings response")
}

pub async fn get_listing_bookings(listing_id: Uuid) -> Result<Vec<BookingResponse>> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/listing/{}", api_url, listing_id);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!(
            "Failed to fetch listing bookings ({}): {}",
            status,
            err_text
        );
    }

    res.json::<Vec<BookingResponse>>()
        .await
        .context("Failed to parse listing bookings response")
}

pub async fn get_all_bookings(
    page: Option<u32>,
    per_page: Option<u32>,
) -> Result<Vec<BookingResponse>> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let page = page.unwrap_or(1);
    let per_page = per_page.unwrap_or(50);
    let url = format!(
        "{}/api/v1/bookings?page={}&per_page={}",
        api_url, page, per_page
    );

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch bookings ({}): {}", status, err_text);
    }

    res.json::<Vec<BookingResponse>>()
        .await
        .context("Failed to parse bookings response")
}

// -----------------------------------------------------------------------------
// Review API Clients
// -----------------------------------------------------------------------------

pub async fn get_review_token_info(token: &str) -> Result<ReviewTokenInfoResponse> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/reviews/token/{}", api_url, token);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to fetch review token info: {}", res.status());
    }

    res.json::<ReviewTokenInfoResponse>()
        .await
        .context("Failed to parse review token info")
}

pub async fn get_booking_review_token(booking_id: Uuid) -> Result<BookingReviewEligibility> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/reviews/booking/{}/token", api_url, booking_id);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to fetch booking review token: {}", res.status());
    }

    res.json::<BookingReviewEligibility>()
        .await
        .context("Failed to parse booking review token")
}

pub async fn submit_review(token: &str, req: &NewReviewRequest) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/reviews/token/{}", api_url, token);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to submit review: {}", res.status());
    }

    Ok(())
}

pub async fn submit_host_reply(review_id: Uuid, req: &HostReplyRequest) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/reviews/{}/reply", api_url, review_id);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to submit host reply: {}", res.status());
    }

    Ok(())
}

pub async fn get_listing_reviews(
    listing_id: Uuid,
    page: i64,
    per_page: i64,
) -> Result<Vec<ReviewResponse>> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!(
        "{}/api/v1/listings/{}/reviews?page={}&per_page={}",
        api_url, listing_id, page, per_page
    );

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to fetch listing reviews: {}", res.status());
    }

    res.json::<Vec<ReviewResponse>>()
        .await
        .context("Failed to parse listing reviews")
}

// -----------------------------------------------------------------------------
// User Profile API Clients
// -----------------------------------------------------------------------------

pub async fn fetch_user_profile(user_id: &str) -> Result<Response> {
    let url = format!("{}/api/v1/users/{}", user_api_url(), user_id);
    let audience = user_api_audience();
    get_client().get(&url, &audience).await
}

pub async fn get_all_users(
    page: Option<u32>,
    per_page: Option<u32>,
    search: Option<String>,
) -> Result<Vec<UserResponse>> {
    let api_url = user_api_url();
    let audience = user_api_audience();
    let page = page.unwrap_or(1);
    let per_page = per_page.unwrap_or(50);
    let mut url = format!(
        "{}/api/v1/users?page={}&per_page={}",
        api_url, page, per_page
    );
    if let Some(s) = search.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&search={}", s));
    }

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to user service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch users ({}): {}", status, err_text);
    }

    res.json::<Vec<UserResponse>>()
        .await
        .context("Failed to parse users response")
}

pub async fn update_user(id: Uuid, req: &UpdateUserRequest) -> Result<UserResponse> {
    let url = format!("{}/api/v1/users/user/{}", user_api_url(), id);
    let audience = user_api_audience();
    let res = get_client()
        .patch(&url, &audience, req)
        .await
        .context("Failed to connect to user service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to update user ({}): {}", status, err_text);
    }

    res.json::<UserResponse>()
        .await
        .context("Failed to parse updated user response")
}

// -----------------------------------------------------------------------------
// Session API Clients (Token-hash backed via user_api)
// -----------------------------------------------------------------------------

pub async fn create_session(req: &CreateSessionRequest) -> Result<SessionResponse> {
    let url = format!("{}/api/v1/sessions", user_api_url());
    let audience = user_api_audience();

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to user service for session creation")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to create session ({}): {}", status, err_text);
    }

    res.json::<SessionResponse>()
        .await
        .context("Failed to parse session creation response")
}

pub async fn get_session(
    token_hash: &str,
    namespace: Option<&str>,
) -> Result<Option<SessionResponse>> {
    let api_url = user_api_url();
    let audience = user_api_audience();
    let mut url = format!("{}/api/v1/sessions/{}", api_url, token_hash);
    if let Some(ns) = namespace {
        url.push_str(&format!("?namespace={}", ns));
    }

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to user service for session lookup")?;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch session ({}): {}", status, err_text);
    }

    let session = res
        .json::<SessionResponse>()
        .await
        .context("Failed to parse session response")?;

    Ok(Some(session))
}

pub async fn delete_session(token_hash: &str) -> Result<()> {
    let url = format!("{}/api/v1/sessions/{}", user_api_url(), token_hash);
    let audience = user_api_audience();

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to user service for session deletion")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete session ({}): {}", status, err_text);
    }

    Ok(())
}

pub async fn revoke_user_sessions(user_id: Uuid, namespace: Option<&str>) -> Result<()> {
    let api_url = user_api_url();
    let audience = user_api_audience();
    let mut url = format!("{}/api/v1/users/{}/sessions", api_url, user_id);
    if let Some(ns) = namespace {
        url.push_str(&format!("?namespace={}", ns));
    }

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to user service for user session revocation")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to revoke user sessions ({}): {}", status, err_text);
    }

    Ok(())
}

pub async fn login_user(req: &LoginRequest) -> Result<UserResponse> {
    let api_url = user_api_url();
    let audience = user_api_audience();
    let url = format!("{}/api/v1/users/login", api_url);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to user service for login")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Login failed ({}): {}", status, err_text);
    }

    let user: UserResponse = res
        .json()
        .await
        .context("Failed to parse user response from login")?;

    Ok(user)
}
