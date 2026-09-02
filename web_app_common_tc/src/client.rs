use chrono::NaiveDate;
use common::models::{
    BookingResponse, CreateSessionRequest, DynamicPricingQuote, ListingDetails, ListingResponse,
    LoginRequest, NewBookingRequest, PriceOverride, ReviewResponse, SessionResponse,
    UpdateUserRequest, UserResponse,
};

pub use common::app_client::ListingSearchParams;
use std::sync::Arc;
use topcoat::context::{Cx, try_app_context};
use uuid::Uuid;

/// Strongly-typed HTTP Client & API Gateway Service registered in Topcoat's `app_context`.
#[derive(Clone)]
pub struct TopcoatApiClient {
    pub client: Arc<common::http_client::AuthenticatedClient>,
    pub listing_api_url: String,
    pub booking_api_url: String,
    pub user_api_url: String,
}

impl std::fmt::Debug for TopcoatApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TopcoatApiClient")
            .field("listing_api_url", &self.listing_api_url)
            .field("booking_api_url", &self.booking_api_url)
            .field("user_api_url", &self.user_api_url)
            .finish()
    }
}

impl Default for TopcoatApiClient {
    fn default() -> Self {
        Self::from_env()
    }
}

impl TopcoatApiClient {
    /// Initialize API client reading service URLs from environment variables
    pub fn from_env() -> Self {
        Self {
            client: Arc::new(common::app_client::get_client().clone()),
            listing_api_url: common::app_client::listing_api_url(),
            booking_api_url: common::app_client::booking_api_url(),
            user_api_url: common::app_client::user_api_url(),
        }
    }

    pub fn new(
        client: common::http_client::AuthenticatedClient,
        listing_api_url: String,
        booking_api_url: String,
        user_api_url: String,
    ) -> Self {
        Self {
            client: Arc::new(client),
            listing_api_url: listing_api_url.trim_end_matches('/').to_string(),
            booking_api_url: booking_api_url.trim_end_matches('/').to_string(),
            user_api_url: user_api_url.trim_end_matches('/').to_string(),
        }
    }

    /// Search listings with optional filtering
    pub async fn search_listings(
        &self,
        params: ListingSearchParams,
    ) -> anyhow::Result<Vec<ListingResponse>> {
        common::app_client::search_listings(params).await
    }

    /// Get listing details by slug or UUID
    pub async fn get_listing_by_id(
        &self,
        id_or_slug: &str,
        currency: Option<&str>,
    ) -> anyhow::Result<ListingDetails> {
        common::app_client::get_listing_by_id(id_or_slug, currency).await
    }

    /// Get reviews for a listing
    pub async fn get_listing_reviews(
        &self,
        listing_id: Uuid,
        page: i64,
        per_page: i64,
    ) -> anyhow::Result<Vec<ReviewResponse>> {
        common::app_client::get_listing_reviews(listing_id, page, per_page).await
    }

    /// Calculate dynamic pricing quote for a stay
    pub async fn get_pricing_quote(
        &self,
        listing_id: Uuid,
        check_in: NaiveDate,
        check_out: NaiveDate,
        currency: Option<&str>,
    ) -> anyhow::Result<DynamicPricingQuote> {
        common::app_client::get_pricing_quote(listing_id, check_in, check_out, currency).await
    }

    /// Create a 15-minute booking hold
    pub async fn create_booking(&self, req: &NewBookingRequest) -> anyhow::Result<BookingResponse> {
        common::app_client::create_booking(req).await
    }

    /// Get all bookings for admin
    pub async fn get_all_bookings(
        &self,
        page: Option<u32>,
        per_page: Option<u32>,
    ) -> anyhow::Result<Vec<BookingResponse>> {
        common::app_client::get_all_bookings(page, per_page).await
    }

    /// Get all registered users for admin
    pub async fn get_all_users(
        &self,
        page: Option<u32>,
        per_page: Option<u32>,
        role: Option<String>,
    ) -> anyhow::Result<Vec<UserResponse>> {
        common::app_client::get_all_users(page, per_page, role).await
    }

    /// Update user credentials and profile for admin
    pub async fn update_user(
        &self,
        id: Uuid,
        req: &UpdateUserRequest,
    ) -> anyhow::Result<UserResponse> {
        common::app_client::update_user(id, req).await
    }

    /// Get seasonal price overrides for a listing
    pub async fn get_price_overrides(
        &self,
        listing_id: Uuid,
    ) -> anyhow::Result<Vec<PriceOverride>> {
        common::app_client::get_price_overrides(listing_id).await
    }

    /// Create and persist session in user_api
    pub async fn create_session(
        &self,
        req: &CreateSessionRequest,
    ) -> anyhow::Result<SessionResponse> {
        common::app_client::create_session(req).await
    }

    /// Retrieve session from user_api
    pub async fn get_session(
        &self,
        token_hash: &str,
        namespace: Option<&str>,
    ) -> anyhow::Result<Option<SessionResponse>> {
        common::app_client::get_session(token_hash, namespace).await
    }

    /// Delete session in user_api
    pub async fn delete_session(&self, token_hash: &str) -> anyhow::Result<()> {
        common::app_client::delete_session(token_hash).await
    }

    /// Revoke all sessions for a user
    pub async fn revoke_user_sessions(
        &self,
        user_id: Uuid,
        namespace: Option<&str>,
    ) -> anyhow::Result<()> {
        common::app_client::revoke_user_sessions(user_id, namespace).await
    }

    /// Authenticate user credentials with user_api
    pub async fn login_user(&self, req: &LoginRequest) -> anyhow::Result<UserResponse> {
        common::app_client::login_user(req).await
    }
}

static DEFAULT_CLIENT: std::sync::OnceLock<TopcoatApiClient> = std::sync::OnceLock::new();

/// Extracts the registered `TopcoatApiClient` from Topcoat's `Cx` app_context.
/// Falls back to environment-configured default instance if not registered in context (useful for isolated unit tests).
pub fn get_api_client(cx: &Cx) -> &TopcoatApiClient {
    if let Some(client) = try_app_context::<TopcoatApiClient>(cx) {
        client
    } else {
        DEFAULT_CLIENT.get_or_init(TopcoatApiClient::from_env)
    }
}
