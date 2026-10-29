use db_core::models::{Booking, StructureType};
use serde::Serialize;

pub use common::models::{BookingMetadataResponse, BookingResponse};

/// Helper to map database listing model to API response DTO.
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
        weekly_discount_percentage: listing.weekly_discount_percentage,
        monthly_discount_percentage: listing.monthly_discount_percentage,
        is_active: listing.is_active,
        added_at: listing.added_at,
        owner_name: None,
        primary_image_url: listing.primary_image_url,
        max_guests: listing.max_guests,
        bedrooms: listing.bedrooms,
        beds: listing.beds,
        full_bathrooms: listing.full_bathrooms,
        half_bathrooms: listing.half_bathrooms,
        square_meters: listing.square_meters,
        latitude: listing.latitude,
        longitude: listing.longitude,
        overall_rating: listing.overall_rating,
        city: listing.city,
        base_currency: listing.base_currency,
        slug: listing.slug.clone(),
        listing_details: Some(listing.listing_details.0),
        minimum_stay: listing.minimum_stay,
        days_between_bookings: listing.days_between_bookings,
        commission_pct: Some(listing.commission_pct),
        cleaning_fee: Some(listing.cleaning_fee),
        damage_deposit: Some(listing.damage_deposit),
        guests_included: listing.guests_included,
        extra_guest_fee: Some(listing.extra_guest_fee),
    }
}

/// Variant shown for a photo once image_worker has resized it.
const DISPLAY_RESOLUTION: db_core::models::ImageResolution =
    db_core::models::ImageResolution::Desktop1920w;

/// One entry per uploaded photo, in display order: its display-size variant,
/// or the original until variants exist (e.g. seeded photos). The raw upload
/// lives in a private bucket in production, and the other sizes would show
/// the same photo again, so neither is listed separately.
pub fn photo_images(
    images: &[db_core::models::ListingImage],
) -> Vec<common::models::ListingImageResponse> {
    images
        .iter()
        .filter(|photo| photo.parent_id.is_none())
        .filter_map(|photo| {
            images
                .iter()
                .find(|v| v.parent_id == Some(photo.id) && v.resolution == DISPLAY_RESOLUTION)
                .and_then(|v| v.upload_url.clone())
                .or_else(|| photo.upload_url.clone())
                .filter(|url| !url.is_empty())
                .map(|url| common::models::ListingImageResponse { id: photo.id, url })
        })
        .collect()
}

pub fn map_listing_details_to_response(
    details: db_core::models::ListingDetails,
) -> common::models::ListingDetails {
    common::models::ListingDetails {
        listing: map_listing_to_response(details.listing),
        images: photo_images(&details.images),
        host_name: details.owner_name,
        rating_summary: details.rating_summary,
    }
}

/// Helper to map database listing with owner details to API response DTO.
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
        weekly_discount_percentage: listing.weekly_discount_percentage,
        monthly_discount_percentage: listing.monthly_discount_percentage,
        is_active: listing.is_active,
        added_at: listing.added_at,
        owner_name: listing.owner_name,
        primary_image_url: listing.primary_image_url,
        max_guests: listing.max_guests,
        bedrooms: listing.bedrooms,
        beds: listing.beds,
        full_bathrooms: listing.full_bathrooms,
        half_bathrooms: listing.half_bathrooms,
        square_meters: listing.square_meters,
        latitude: listing.latitude,
        longitude: listing.longitude,
        overall_rating: listing.overall_rating,
        city: listing.city,
        base_currency: listing.base_currency,
        slug: listing.slug.clone(),
        listing_details: Some(listing.listing_details.0),
        minimum_stay: listing.minimum_stay,
        days_between_bookings: listing.days_between_bookings,
        commission_pct: Some(listing.commission_pct),
        cleaning_fee: Some(listing.cleaning_fee),
        damage_deposit: Some(listing.damage_deposit),
        guests_included: listing.guests_included,
        extra_guest_fee: Some(listing.extra_guest_fee),
    }
}

/// XML collection wrapper for listings responses.
#[derive(Serialize)]
#[serde(rename = "listings")]
pub struct ListingsWrapper<T> {
    /// Inner list of serialized listing records.
    pub listing: Vec<T>,
}

/// XML collection wrapper for bookings responses.
#[derive(Serialize)]
#[serde(rename = "bookings")]
pub struct BookingsWrapper<T> {
    /// Inner list of serialized booking records.
    pub booking: Vec<T>,
}

/// Helper to map database booking model to API response DTO.
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
            business_company_name: None,
            business_tax_id: None,
        },
        review_eligibility: None,
        door_access_code: booking.door_access_code,
        cleaning_fee: Some(booking.cleaning_fee),
        damage_deposit: Some(booking.damage_deposit),
        platform_fee: Some(booking.platform_fee),
        tax_rate: Some(booking.tax_rate),
        exchange_rate: Some(booking.exchange_rate),
        created_at: booking.created_at,
        updated_at: booking.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use db_core::models::{ImageResolution, ImageStatus, ListingImage};
    use uuid::Uuid;

    fn image(
        id: Uuid,
        parent: Option<Uuid>,
        resolution: ImageResolution,
        url: &str,
    ) -> ListingImage {
        ListingImage {
            id,
            listing_id: Uuid::nil(),
            client_file_id: "0".into(),
            status: ImageStatus::Processed,
            resolution,
            parent_id: parent,
            upload_url: Some(url.into()),
            content_type: None,
            size_bytes: None,
            display_order: 0,
            is_primary: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn a_resized_photo_is_listed_once_at_display_size() {
        let photo = Uuid::now_v7();
        let images = [
            image(photo, None, ImageResolution::Raw, "raw/photo"),
            image(
                Uuid::now_v7(),
                Some(photo),
                ImageResolution::Thumbnail400w,
                "thumb",
            ),
            image(
                Uuid::now_v7(),
                Some(photo),
                ImageResolution::Desktop1920w,
                "desktop",
            ),
            image(
                Uuid::now_v7(),
                Some(photo),
                ImageResolution::HighRes2560w,
                "highres",
            ),
        ];
        let listed = photo_images(&images);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, photo);
        assert_eq!(listed[0].url, "desktop");
    }

    #[test]
    fn a_photo_without_variants_shows_its_original() {
        let photo = Uuid::now_v7();
        let listed = photo_images(&[image(photo, None, ImageResolution::Raw, "seeded.jpg")]);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].url, "seeded.jpg");
    }

    #[test]
    fn photos_keep_their_order_and_variants_alone_are_not_listed() {
        let (first, second) = (Uuid::now_v7(), Uuid::now_v7());
        let images = [
            image(first, None, ImageResolution::Raw, "first"),
            image(
                Uuid::now_v7(),
                Some(Uuid::now_v7()),
                ImageResolution::Desktop1920w,
                "orphan",
            ),
            image(second, None, ImageResolution::Raw, "second"),
        ];
        let urls: Vec<_> = photo_images(&images).into_iter().map(|i| i.url).collect();
        assert_eq!(urls, ["first", "second"]);
    }
}
