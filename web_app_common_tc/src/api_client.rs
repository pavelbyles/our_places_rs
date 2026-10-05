use chrono::NaiveDate;
pub use common::app_client::*;
use common::models::{
    BookingResponse, DynamicPricingQuote, ListingDetails, ListingResponse, NewBookingRequest,
};
use uuid::Uuid;

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

/// Calculate dynamic pricing quote via booking_api
pub async fn get_pricing_quote_tc(
    listing_id: Uuid,
    check_in: NaiveDate,
    check_out: NaiveDate,
    currency: Option<&str>,
) -> anyhow::Result<DynamicPricingQuote> {
    common::app_client::get_pricing_quote(listing_id, check_in, check_out, currency).await
}

/// Create a 15-minute booking hold via booking_api
pub async fn create_booking_hold_tc(req: &NewBookingRequest) -> anyhow::Result<BookingResponse> {
    common::app_client::create_booking(req).await
}
