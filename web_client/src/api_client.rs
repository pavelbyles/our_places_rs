pub use common::app_client::*;
use common::models::{BookingResponse, ListingDetails, ListingResponse, NewBookingRequest};

/// Search listings with parameters via listing_api
pub async fn search_listings_tc(
    params: ListingSearchParams,
) -> anyhow::Result<Vec<ListingResponse>> {
    common::app_client::search_listings(params).await
}

/// Get listing details by ID via listing_api
pub async fn get_listing_details_tc(
    id: &str,
    currency: Option<&str>,
) -> anyhow::Result<ListingDetails> {
    common::app_client::get_listing_by_id(id, currency).await
}

/// Create a pending booking hold via booking_api, as the guest who owns `jwt`
pub async fn create_booking_hold_tc(
    req: &NewBookingRequest,
    jwt: &str,
) -> anyhow::Result<BookingResponse> {
    common::app_client::create_booking(req, jwt).await
}
