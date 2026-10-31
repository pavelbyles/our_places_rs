use chrono::NaiveDate;
use common::models::{
    BookingMessageResponse, BookingMessagesWrapper, BookingResponse, CreateBookingMessageRequest,
    CreateSessionRequest, DynamicPricingQuote, ListingDetails, ListingResponse, LoginRequest,
    MarkMessagesReadResponse, NewBookingRequest, NewUserRequest, PriceOverride,
    ResendVerificationRequest, ReviewResponse, SessionResponse, UpdateUserRequest,
    UpdatedBookingRequest, UserResponse, VerifyRequest,
};

pub use common::app_client::ListingSearchParams;
use common::app_client::{booking_api_audience, user_api_audience};
use common::http_client::AuthenticatedClient;
use std::sync::Arc;
use topcoat::context::{Cx, try_app_context};
use uuid::Uuid;

/// Strongly-typed HTTP Client & API Gateway Service registered in Topcoat's `app_context`.
#[derive(Clone)]
pub struct TopcoatApiClient {
    /// Shared authenticated HTTP client used for every backend call.
    pub client: Arc<common::http_client::AuthenticatedClient>,
    /// Base URL of `listing_api`.
    pub listing_api_url: String,
    /// Base URL of `booking_api`.
    pub booking_api_url: String,
    /// Base URL of `user_api`.
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

    /// Builds a client from explicit service URLs, trimming trailing slashes.
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

    /// Create a new listing
    pub async fn create_listing(
        &self,
        req: &common::models::NewListingRequest,
    ) -> anyhow::Result<common::models::ListingResponse> {
        common::app_client::create_listing(req).await
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

    /// Update a booking
    pub async fn update_booking(
        &self,
        id: Uuid,
        req: &UpdatedBookingRequest,
    ) -> anyhow::Result<BookingResponse> {
        common::app_client::update_booking(id, req).await
    }

    /// Update booking with optional user JWT token
    pub async fn update_booking_with_auth(
        &self,
        id: Uuid,
        req: &UpdatedBookingRequest,
        user_jwt: Option<&str>,
    ) -> anyhow::Result<BookingResponse> {
        common::app_client::update_booking_with_auth(id, req, user_jwt).await
    }

    /// Get cancellation quote
    pub async fn get_cancellation_quote(
        &self,
        id: Uuid,
    ) -> anyhow::Result<common::cancellation::RefundQuote> {
        common::app_client::get_cancellation_quote(id).await
    }

    /// Get booking messages
    pub async fn get_booking_messages(&self, id: Uuid) -> anyhow::Result<BookingMessagesWrapper> {
        common::app_client::get_booking_messages(id).await
    }

    /// Send a booking message
    pub async fn send_booking_message(
        &self,
        id: Uuid,
        req: &CreateBookingMessageRequest,
    ) -> anyhow::Result<BookingMessageResponse> {
        common::app_client::send_booking_message(id, req).await
    }

    /// Mark booking messages as read
    pub async fn mark_booking_messages_read(
        &self,
        id: Uuid,
    ) -> anyhow::Result<MarkMessagesReadResponse> {
        common::app_client::mark_booking_messages_read(id).await
    }

    /// Retrieve a specific statutory tax document or receipt by number
    pub async fn get_booking_document_by_number(
        &self,
        booking_id: Uuid,
        document_number: &str,
        user_jwt: Option<&str>,
    ) -> anyhow::Result<common::tax_document::TaxDocumentResponse> {
        common::app_client::get_booking_document_by_number(booking_id, document_number, user_jwt)
            .await
    }

    /// List all statutory tax documents for a booking
    pub async fn list_booking_documents(
        &self,
        booking_id: Uuid,
        user_jwt: Option<&str>,
    ) -> anyhow::Result<Vec<common::tax_document::TaxDocumentResponse>> {
        common::app_client::list_booking_documents(booking_id, user_jwt).await
    }

    /// Quote a reservation change (dates & guest counts)
    pub async fn quote_reservation_change(
        &self,
        booking_id: Uuid,
        req: &common::models::ReservationChangeQuoteRequest,
        user_jwt: Option<&str>,
    ) -> anyhow::Result<common::models::ReservationChangeQuoteResponse> {
        common::app_client::quote_reservation_change(booking_id, req, user_jwt).await
    }

    /// Create a bilateral reservation change request
    pub async fn create_reservation_change_request(
        &self,
        booking_id: Uuid,
        req: &common::models::CreateReservationChangeRequest,
        user_jwt: Option<&str>,
    ) -> anyhow::Result<common::models::ReservationChangeRequestResponse> {
        common::app_client::create_reservation_change_request(booking_id, req, user_jwt).await
    }

    /// List reservation change requests for a booking
    pub async fn list_reservation_change_requests(
        &self,
        booking_id: Uuid,
        user_jwt: Option<&str>,
    ) -> anyhow::Result<Vec<common::models::ReservationChangeRequestResponse>> {
        common::app_client::list_reservation_change_requests(booking_id, user_jwt).await
    }

    /// Approve a pending reservation change request
    pub async fn approve_reservation_change_request(
        &self,
        booking_id: Uuid,
        request_id: Uuid,
        req: &common::models::RespondReservationChangeRequest,
        user_jwt: Option<&str>,
    ) -> anyhow::Result<common::models::ReservationChangeRequestResponse> {
        common::app_client::approve_reservation_change_request(
            booking_id, request_id, req, user_jwt,
        )
        .await
    }

    /// Decline a pending reservation change request
    pub async fn decline_reservation_change_request(
        &self,
        booking_id: Uuid,
        request_id: Uuid,
        req: &common::models::RespondReservationChangeRequest,
        user_jwt: Option<&str>,
    ) -> anyhow::Result<common::models::ReservationChangeRequestResponse> {
        common::app_client::decline_reservation_change_request(
            booking_id, request_id, req, user_jwt,
        )
        .await
    }

    /// Withdraw (cancel) a pending reservation change request
    pub async fn cancel_reservation_change_request(
        &self,
        booking_id: Uuid,
        request_id: Uuid,
        user_jwt: Option<&str>,
    ) -> anyhow::Result<common::models::ReservationChangeRequestResponse> {
        common::app_client::cancel_reservation_change_request(booking_id, request_id, user_jwt)
            .await
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

    /// Get all users with optional pagination, search filtering, and soft-delete filtering
    pub async fn get_all_users_filtered(
        &self,
        page: Option<u32>,
        per_page: Option<u32>,
        search: Option<String>,
        is_deleted: Option<bool>,
    ) -> anyhow::Result<Vec<UserResponse>> {
        // The shared client puts `search` into the URL as-is.
        let search = search.map(|s| encode_query_value(&s));
        common::app_client::get_all_users_filtered(page, per_page, search, is_deleted).await
    }

    /// Soft deletes a user account (marks as deleted and deactivated)
    pub async fn soft_delete_user(&self, id: Uuid) -> anyhow::Result<UserResponse> {
        common::app_client::soft_delete_user(id).await
    }

    /// Permanently purges a user from the database
    pub async fn hard_delete_user(&self, id: Uuid) -> anyhow::Result<()> {
        common::app_client::hard_delete_user(id).await
    }

    /// Restores an archived / soft-deleted user
    pub async fn restore_user(&self, id: Uuid) -> anyhow::Result<UserResponse> {
        common::app_client::restore_user(id).await
    }

    /// Fetch a single user by ID
    pub async fn get_user_by_id(&self, id: Uuid) -> anyhow::Result<UserResponse> {
        let res = common::app_client::fetch_user_profile(&id.to_string()).await?;
        if !res.status().is_success() {
            anyhow::bail!("Failed to fetch user {}: status {}", id, res.status());
        }
        res.json::<UserResponse>()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse user: {}", e))
    }

    /// Create user account for admin
    pub async fn create_user(&self, req: &NewUserRequest) -> anyhow::Result<UserResponse> {
        common::app_client::create_user(req).await
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

    /// Verifies a user's email address with their 6-character OTP code
    pub async fn verify_user(&self, req: &VerifyRequest) -> anyhow::Result<UserResponse> {
        common::app_client::verify_user(req).await
    }

    /// Resends the 6-character OTP verification code to a user's email address
    pub async fn resend_verification(
        &self,
        req: &ResendVerificationRequest,
    ) -> anyhow::Result<UserResponse> {
        common::app_client::resend_verification(req).await
    }

    /// Retrieve host payout ledger entries
    pub async fn get_host_payout_ledger(
        &self,
        filter: &common::payout::PayoutFilter,
    ) -> anyhow::Result<common::payout::PayoutLedgerResponse> {
        common::app_client::get_host_payout_ledger(filter).await
    }

    /// Retrieve host payout summary statistics
    pub async fn get_host_payout_summary(
        &self,
        filter: &common::payout::PayoutFilter,
    ) -> anyhow::Result<common::payout::PayoutSummary> {
        common::app_client::get_host_payout_summary(filter).await
    }

    /// Update payout status (admin only)
    pub async fn update_admin_payout_status(
        &self,
        id: Uuid,
        req: &common::payout::UpdatePayoutStatusRequest,
    ) -> anyhow::Result<common::payout::PayoutLedgerEntry> {
        common::app_client::update_admin_payout_status(id, req).await
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

/// Why a backend call made on behalf of a user failed.
#[derive(Debug, thiserror::Error)]
pub enum UserCallError {
    /// 404: the resource does not exist, or the user may not see it (the
    /// backend answers 404 to strangers to prevent ID enumeration).
    #[error("not found")]
    NotFound,
    /// 401/403: the user token was missing, invalid or not allowed.
    #[error("unauthorized")]
    Unauthorized,
    /// Any other non-success status.
    #[error("backend returned status {0}")]
    Status(u16),
    /// The request could not be sent or the body could not be decoded.
    #[error("backend request failed: {0}")]
    Transport(#[from] reqwest::Error),
    /// No Google ID token could be obtained for the backend (Cloud Run IAM).
    #[error("could not get a service identity token: {0}")]
    Identity(anyhow::Error),
}

impl TopcoatApiClient {
    /// Lists the bookings of `user_id`, authenticated as that user.
    pub async fn get_user_bookings_as(
        &self,
        user_id: Uuid,
        jwt: &str,
    ) -> Result<Vec<BookingResponse>, UserCallError> {
        let url = format!("{}/api/v1/bookings/user/{user_id}", self.booking_api_url);
        get_json_as(&self.client, &url, &booking_api_audience(), jwt).await
    }

    /// Fetches one booking, authenticated as the user who owns `jwt`.
    pub async fn get_booking_as(
        &self,
        id: Uuid,
        jwt: &str,
    ) -> Result<BookingResponse, UserCallError> {
        let url = format!("{}/api/v1/bookings/{id}", self.booking_api_url);
        get_json_as(&self.client, &url, &booking_api_audience(), jwt).await
    }

    /// Fetches a booking's cancellation refund quote, authenticated as the
    /// user who owns `jwt`.
    pub async fn get_cancellation_quote_as(
        &self,
        id: Uuid,
        jwt: &str,
    ) -> Result<common::cancellation::RefundQuote, UserCallError> {
        let url = format!(
            "{}/api/v1/bookings/{id}/cancellation-quote",
            self.booking_api_url
        );
        get_json_as(&self.client, &url, &booking_api_audience(), jwt).await
    }
}

impl TopcoatApiClient {
    /// A booking's message thread, as the user who owns `jwt` (guest, host or
    /// admin of that booking).
    pub async fn get_booking_messages_as(
        &self,
        id: Uuid,
        jwt: &str,
    ) -> Result<BookingMessagesWrapper, UserCallError> {
        let url = format!("{}/api/v1/bookings/{id}/messages", self.booking_api_url);
        get_json_as(&self.client, &url, &booking_api_audience(), jwt).await
    }

    /// Posts a message to a booking's thread as the user who owns `jwt`.
    pub async fn send_booking_message_as(
        &self,
        id: Uuid,
        text: &str,
        jwt: &str,
    ) -> Result<BookingMessageResponse, UserCallError> {
        let url = format!("{}/api/v1/bookings/{id}/messages", self.booking_api_url);
        let body = CreateBookingMessageRequest {
            message_text: text.to_string(),
        };
        send_json_as(
            &self.client,
            reqwest::Method::POST,
            &url,
            &booking_api_audience(),
            jwt,
            &body,
        )
        .await
    }

    /// Marks the other party's messages read, as the user who owns `jwt`.
    pub async fn mark_booking_messages_read_as(
        &self,
        id: Uuid,
        jwt: &str,
    ) -> Result<MarkMessagesReadResponse, UserCallError> {
        let url = format!(
            "{}/api/v1/bookings/{id}/messages/read",
            self.booking_api_url
        );
        send_json_as(
            &self.client,
            reqwest::Method::PATCH,
            &url,
            &booking_api_audience(),
            jwt,
            &serde_json::json!({}),
        )
        .await
    }
}

impl TopcoatApiClient {
    /// One user's account, from `user_api`'s `/api/v1/users/user/{id}`.
    pub async fn get_user_profile(&self, id: Uuid) -> Result<UserResponse, UserCallError> {
        let url = format!("{}/api/v1/users/user/{id}", self.user_api_url);
        let request = service_request(
            &self.client,
            reqwest::Method::GET,
            &url,
            &user_api_audience(),
            None,
        )
        .await?;
        decode(request.send().await?).await
    }
}

impl TopcoatApiClient {
    /// Asks `listing_api` for GCP V4 signed upload URLs for new listing
    /// photos (the browser then uploads straight to Cloud Storage).
    pub async fn presign_listing_images(
        &self,
        listing_id: Uuid,
        req: &common::models::ImagePresignRequest,
    ) -> anyhow::Result<Vec<common::models::ImagePresignResponse>> {
        let url = format!(
            "{}/api/v1/listings/{listing_id}/images/presign",
            self.listing_api_url
        );
        let audience = common::app_client::listing_api_audience();
        let res = self.client.post(&url, &audience, req).await?;
        if !res.status().is_success() {
            anyhow::bail!("presign failed with status {}", res.status());
        }
        Ok(res.json().await?)
    }
}

impl TopcoatApiClient {
    /// Overrides a payout's status as the admin who owns `jwt` (`booking_api`
    /// requires the admin role; the legacy call sent no user token).
    pub async fn update_payout_status_as(
        &self,
        id: Uuid,
        req: &common::payout::UpdatePayoutStatusRequest,
        jwt: &str,
    ) -> Result<common::payout::PayoutLedgerEntry, UserCallError> {
        let url = format!("{}/api/v1/admin/ledger/{id}/status", self.booking_api_url);
        send_json_as(
            &self.client,
            reqwest::Method::PATCH,
            &url,
            &booking_api_audience(),
            jwt,
            req,
        )
        .await
    }

    /// The newest rate per currency pair, as the admin or host who owns `jwt`.
    pub async fn get_exchange_rates_as(
        &self,
        jwt: &str,
    ) -> Result<common::exchange_rate::ExchangeRatesResponse, UserCallError> {
        let url = format!("{}/api/v1/admin/exchange-rates", self.booking_api_url);
        get_json_as(&self.client, &url, &booking_api_audience(), jwt).await
    }
}

/// A request carrying the service identity token (Cloud Run IAM), the gateway
/// API key if configured, and the user's JWT when given.
async fn service_request(
    client: &AuthenticatedClient,
    method: reqwest::Method,
    url: &str,
    audience: &str,
    jwt: Option<&str>,
) -> Result<reqwest::RequestBuilder, UserCallError> {
    client
        .user_request(method, url, audience, jwt)
        .await
        .map_err(UserCallError::Identity)
}

/// Sends a JSON body as the user who owns `jwt` and decodes a JSON response.
async fn send_json_as<B: serde::Serialize + ?Sized, T: serde::de::DeserializeOwned>(
    client: &AuthenticatedClient,
    method: reqwest::Method,
    url: &str,
    audience: &str,
    jwt: &str,
    body: &B,
) -> Result<T, UserCallError> {
    let request = service_request(client, method, url, audience, Some(jwt)).await?;
    decode(request.json(body).send().await?).await
}

async fn decode<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, UserCallError> {
    match response.status().as_u16() {
        200..=299 => Ok(response.json::<T>().await?),
        404 => Err(UserCallError::NotFound),
        401 | 403 => Err(UserCallError::Unauthorized),
        status => Err(UserCallError::Status(status)),
    }
}

/// Sends a GET as the user who owns `jwt` and decodes a JSON body.
async fn get_json_as<T: serde::de::DeserializeOwned>(
    client: &AuthenticatedClient,
    url: &str,
    audience: &str,
    jwt: &str,
) -> Result<T, UserCallError> {
    let request = service_request(client, reqwest::Method::GET, url, audience, Some(jwt)).await?;
    decode(request.send().await?).await
}

impl TopcoatApiClient {
    /// Booked date ranges for a listing within `[from, to)`. Public data
    /// (dates only), so no user token is sent.
    pub async fn get_unavailable_ranges(
        &self,
        listing_id: Uuid,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    ) -> Result<Vec<common::availability::UnavailableRange>, UserCallError> {
        let url = format!(
            "{}/api/v1/bookings/listing/{listing_id}/unavailable-ranges?from={from}&to={to}",
            self.booking_api_url
        );
        let request = service_request(
            &self.client,
            reqwest::Method::GET,
            &url,
            &booking_api_audience(),
            None,
        )
        .await?;
        decode::<common::availability::UnavailableRangesResponse>(request.send().await?)
            .await
            .map(|body| body.ranges)
    }
}

/// Percent-encodes everything except RFC 3986 unreserved characters.
fn encode_query_value(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::encode_query_value;

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(encode_query_value("a.b+c@x.com"), "a.b%2Bc%40x.com");
        assert_eq!(encode_query_value("x&per_page=1000"), "x%26per_page%3D1000");
        assert_eq!(encode_query_value("José"), "Jos%C3%A9");
    }
}
