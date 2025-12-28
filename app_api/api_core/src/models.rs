use chrono::{DateTime, NaiveDate, Utc};
use db_core::models::{Booking, BookingStatus, CancellationPolicy, FeeItem, StructureType};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BookingResponse {
    pub id: Uuid,
    pub confirmation_code: String,
    pub guest_id: Uuid,
    pub listing_id: Uuid,
    pub status: BookingStatus,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    pub currency: String,
    #[serde(with = "rust_decimal::serde::float")]
    pub daily_rate: Decimal,
    pub number_of_persons: i32,
    pub total_days: i32,
    #[serde(with = "rust_decimal::serde::float")]
    pub sub_total_price: Decimal,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub discount_value: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub tax_value: Option<Decimal>,
    pub fee_breakdown: Vec<FeeItem>,
    #[serde(with = "rust_decimal::serde::float")]
    pub total_price: Decimal,
    pub cancellation_policy: CancellationPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Wrapper for XML collections
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListingResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub name: String,
    pub description: Option<String>,

    #[schema(value_type = String, example = "Apartment")]
    pub listing_structure: StructureType,
    pub country: String,

    #[schema(value_type = Option<Decimal>, example = 150.00)]
    #[serde(with = "rust_decimal::serde::float_option")]
    pub price_per_night: Option<Decimal>,
    pub is_active: bool,

    #[schema(value_type = String, format = "date-time")]
    pub added_at: DateTime<Utc>,
}

// Helper to map DB Listing to API Response
pub fn map_listing_to_response(listing: db_core::models::Listing) -> ListingResponse {
    let structure = match listing.listing_structure_id {
        1 => StructureType::Apartment,
        2 => StructureType::House,
        3 => StructureType::Townhouse,
        4 => StructureType::Studio,
        5 => StructureType::Villa,
        _ => StructureType::Apartment, // Fallback
    };

    ListingResponse {
        id: listing.id,
        user_id: listing.user_id,
        name: listing.name,
        description: listing.description,
        listing_structure: structure,
        country: listing.country,
        price_per_night: listing.price_per_night,
        is_active: listing.is_active,
        added_at: listing.added_at,
    }
}

// Wrapper for XML collections
#[derive(Serialize)]
#[serde(rename = "listings")]
pub struct ListingsWrapper<T> {
    pub listing: Vec<T>,
}

// Wrapper for XML collections
#[derive(Serialize)]
#[serde(rename = "bookings")]
pub struct BookingsWrapper<T> {
    pub booking: Vec<T>,
}

pub fn map_booking_to_response(booking: Booking) -> BookingResponse {
    BookingResponse {
        id: booking.id,
        confirmation_code: booking.confirmation_code,
        guest_id: booking.guest_id,
        listing_id: booking.listing_id,
        status: booking.status,
        date_from: booking.date_from,
        date_to: booking.date_to,
        currency: booking.currency,
        daily_rate: booking.daily_rate,
        number_of_persons: booking.number_of_persons,
        total_days: booking.total_days,
        sub_total_price: booking.sub_total_price,
        discount_value: booking.discount_value,
        tax_value: booking.tax_value,
        fee_breakdown: booking.fee_breakdown.0,
        total_price: booking.total_price,
        cancellation_policy: booking.cancellation_policy,
        created_at: booking.created_at,
        updated_at: booking.updated_at,
    }
}
