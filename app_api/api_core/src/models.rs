use db_core::models::{Booking, StructureType};
use serde::Serialize;

pub use common::models::{BookingMetadataResponse, BookingResponse};

// Helper to map DB Listing to API Response
pub fn map_listing_to_response(
    listing: db_core::models::Listing,
) -> common::models::ListingResponse {
    let structure = match listing.listing_structure_id {
        1 => StructureType::Apartment,
        2 => StructureType::House,
        3 => StructureType::Townhouse,
        4 => StructureType::Studio,
        5 => StructureType::Villa,
        _ => StructureType::Apartment, // Fallback
    };

    common::models::ListingResponse {
        id: listing.id,
        user_id: listing.user_id,
        name: listing.name,
        description: listing.description,
        listing_structure: format!("{:?}", structure), // Convert enum to String for common DTO
        country: listing.country,
        price_per_night: listing.price_per_night,
        is_active: listing.is_active,
        added_at: listing.added_at,
        owner_name: None,
        primary_image_url: listing.primary_image_url,
        max_guests: listing.max_guests,
        bedrooms: listing.bedrooms,
        full_bathrooms: listing.full_bathrooms,
        latitude: listing.latitude,
        longitude: listing.longitude,
        overall_rating: listing.overall_rating,
        city: listing.city,
        base_currency: listing.base_currency,
        slug: listing.slug.clone(),
        listing_details: Some(listing.listing_details.0),
        minimum_stay: listing.minimum_stay,
        days_between_bookings: listing.days_between_bookings,
    }
}

pub fn map_listing_details_to_response(
    details: db_core::models::ListingDetails,
) -> common::models::ListingDetails {
    common::models::ListingDetails {
        listing: map_listing_to_response(details.listing),
        images: details
            .images
            .into_iter()
            .map(|img| common::models::ListingImageResponse {
                id: img.id,
                url: img.upload_url.unwrap_or_default(),
            })
            .collect(),
        host_name: details.owner_name,
        rating_summary: details.rating_summary,
    }
}

pub fn map_listing_with_owner_to_response(
    listing: db_core::models::ListingWithOwner,
) -> common::models::ListingResponse {
    let structure = match listing.listing_structure_id {
        1 => StructureType::Apartment,
        2 => StructureType::House,
        3 => StructureType::Townhouse,
        4 => StructureType::Studio,
        5 => StructureType::Villa,
        _ => StructureType::Apartment, // Fallback
    };

    common::models::ListingResponse {
        id: listing.id,
        user_id: listing.user_id,
        name: listing.name,
        description: listing.description,
        listing_structure: format!("{:?}", structure), // Convert enum to String for common DTO
        country: listing.country,
        price_per_night: listing.price_per_night,
        is_active: listing.is_active,
        added_at: listing.added_at,
        owner_name: listing.owner_name,
        primary_image_url: listing.primary_image_url,
        max_guests: listing.max_guests,
        bedrooms: listing.bedrooms,
        full_bathrooms: listing.full_bathrooms,
        latitude: listing.latitude,
        longitude: listing.longitude,
        overall_rating: listing.overall_rating,
        city: listing.city,
        base_currency: listing.base_currency,
        slug: listing.slug.clone(),
        listing_details: Some(listing.listing_details.0),
        minimum_stay: listing.minimum_stay,
        days_between_bookings: listing.days_between_bookings,
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
        status: format!("{:?}", booking.status),
        date_from: booking.date_from,
        date_to: booking.date_to,
        currency: booking.currency,
        daily_rate: booking.daily_rate,
        number_of_persons: booking.number_of_persons,
        total_days: booking.total_days,
        sub_total_price: booking.sub_total_price,
        discount_value: booking.discount_value,
        tax_value: booking.tax_value,
        total_price: booking.total_price,
        cancellation_policy: format!("{:?}", booking.cancellation_policy),
        metadata: BookingMetadataResponse {
            num_adults: booking.metadata.num_adults,
            num_children: booking.metadata.num_children,
            num_infants: booking.metadata.num_infants,
            num_pets: booking.metadata.num_pets,
            message_to_host: booking.metadata.message_to_host.clone(),
            estimated_arrival_time: booking.metadata.estimated_arrival_time.clone(),
            is_business_trip: booking.metadata.is_business_trip,
        },
        created_at: booking.created_at,
        updated_at: booking.updated_at,
    }
}
