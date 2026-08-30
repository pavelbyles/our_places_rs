use chrono::Utc;
use common::models::{
    ListingDetails, ListingImageResponse, ListingRatingSummary, ListingResponse, ReviewResponse,
};
use rust_decimal_macros::dec;
use std::collections::HashMap;
use uuid::Uuid;

pub fn get_sample_listings() -> Vec<ListingResponse> {
    vec![
        ListingResponse {
            id: Uuid::parse_str("018e0000-0000-7000-8000-000000000001").unwrap(),
            user_id: Uuid::parse_str("018e0000-0000-7000-8000-000000000099").unwrap(),
            name: "Villa Serenity — Montego Bay".to_string(),
            description: Some("Experience ultimate Caribbean luxury in this private oceanfront villa featuring an infinity pool, private beach access, and full concierge service.".to_string()),
            listing_structure: "Villa".to_string(),
            country: "Jamaica".to_string(),
            city: Some("Montego Bay".to_string()),
            price_per_night: Some(dec!(650.00)),
            weekly_discount_percentage: Some(dec!(10.0)),
            monthly_discount_percentage: Some(dec!(20.0)),
            is_active: true,
            added_at: Utc::now(),
            owner_name: Some("Pavel & Partners".to_string()),
            primary_image_url: Some("https://images.unsplash.com/photo-1580587771525-78b9dba3b914?auto=format&fit=crop&w=1200&q=80".to_string()),
            max_guests: 10,
            bedrooms: 5,
            beds: 6,
            full_bathrooms: 5,
            half_bathrooms: 1,
            square_meters: Some(450),
            latitude: Some(18.5126),
            longitude: Some(-77.8423),
            overall_rating: Some(4.95),
            base_currency: "USD".to_string(),
            slug: "villa-serenity-montego-bay".to_string(),
            listing_details: None,
            minimum_stay: 3,
            days_between_bookings: 1,
        },
        ListingResponse {
            id: Uuid::parse_str("018e0000-0000-7000-8000-000000000002").unwrap(),
            user_id: Uuid::parse_str("018e0000-0000-7000-8000-000000000099").unwrap(),
            name: "Blue Lagoon Sanctuary — Port Antonio".to_string(),
            description: Some("Nestled within the lush rainforest above the turquoise Blue Lagoon, this open-air villa offers secluded tranquil luxury.".to_string()),
            listing_structure: "Villa".to_string(),
            country: "Jamaica".to_string(),
            city: Some("Port Antonio".to_string()),
            price_per_night: Some(dec!(520.00)),
            weekly_discount_percentage: Some(dec!(5.0)),
            monthly_discount_percentage: Some(dec!(15.0)),
            is_active: true,
            added_at: Utc::now(),
            owner_name: Some("Pavel & Partners".to_string()),
            primary_image_url: Some("https://images.unsplash.com/photo-1512917774080-9991f1c4c750?auto=format&fit=crop&w=1200&q=80".to_string()),
            max_guests: 8,
            bedrooms: 4,
            beds: 4,
            full_bathrooms: 4,
            half_bathrooms: 0,
            square_meters: Some(380),
            latitude: Some(18.1725),
            longitude: Some(-76.3814),
            overall_rating: Some(4.88),
            base_currency: "USD".to_string(),
            slug: "blue-lagoon-sanctuary-port-antonio".to_string(),
            listing_details: None,
            minimum_stay: 2,
            days_between_bookings: 1,
        },
        ListingResponse {
            id: Uuid::parse_str("018e0000-0000-7000-8000-000000000003").unwrap(),
            user_id: Uuid::parse_str("018e0000-0000-7000-8000-000000000099").unwrap(),
            name: "Negril Sunset Cliffside Estate".to_string(),
            description: Some("Perched dramatically above the Caribbean Sea on the West End cliffs of Negril, famous for world-class sunset views and snorkeling caves.".to_string()),
            listing_structure: "Villa".to_string(),
            country: "Jamaica".to_string(),
            city: Some("Negril".to_string()),
            price_per_night: Some(dec!(480.00)),
            weekly_discount_percentage: Some(dec!(8.0)),
            monthly_discount_percentage: Some(dec!(18.0)),
            is_active: true,
            added_at: Utc::now(),
            owner_name: Some("Pavel & Partners".to_string()),
            primary_image_url: Some("https://images.unsplash.com/photo-1613490493576-7fde63acd811?auto=format&fit=crop&w=1200&q=80".to_string()),
            max_guests: 6,
            bedrooms: 3,
            beds: 3,
            full_bathrooms: 3,
            half_bathrooms: 1,
            square_meters: Some(300),
            latitude: Some(18.2568),
            longitude: Some(-78.3621),
            overall_rating: Some(5.0),
            base_currency: "USD".to_string(),
            slug: "negril-sunset-cliffside-estate".to_string(),
            listing_details: None,
            minimum_stay: 2,
            days_between_bookings: 1,
        },
        ListingResponse {
            id: Uuid::parse_str("018e0000-0000-7000-8000-000000000004").unwrap(),
            user_id: Uuid::parse_str("018e0000-0000-7000-8000-000000000099").unwrap(),
            name: "Ocho Rios Coastal Haven".to_string(),
            description: Some("Direct beachfront estate walking distance to Dunn's River Falls, private chef on staff, tropical gardens, and private tennis court.".to_string()),
            listing_structure: "Villa".to_string(),
            country: "Jamaica".to_string(),
            city: Some("Ocho Rios".to_string()),
            price_per_night: Some(dec!(850.00)),
            weekly_discount_percentage: Some(dec!(10.0)),
            monthly_discount_percentage: Some(dec!(25.0)),
            is_active: true,
            added_at: Utc::now(),
            owner_name: Some("Pavel & Partners".to_string()),
            primary_image_url: Some("https://images.unsplash.com/photo-1600596542815-ffad4c1539a9?auto=format&fit=crop&w=1200&q=80".to_string()),
            max_guests: 12,
            bedrooms: 6,
            beds: 8,
            full_bathrooms: 6,
            half_bathrooms: 2,
            square_meters: Some(600),
            latitude: Some(18.4074),
            longitude: Some(-77.1031),
            overall_rating: Some(4.92),
            base_currency: "USD".to_string(),
            slug: "ocho-rios-coastal-haven".to_string(),
            listing_details: None,
            minimum_stay: 3,
            days_between_bookings: 1,
        },
        ListingResponse {
            id: Uuid::parse_str("018e0000-0000-7000-8000-000000000005").unwrap(),
            user_id: Uuid::parse_str("018e0000-0000-7000-8000-000000000099").unwrap(),
            name: "Kingston Skyline Luxury Penthouse".to_string(),
            description: Some("Ultra-modern penthouse overlooking the Blue Mountains and Kingston Harbour. Rooftop plunge pool, 24/7 security, and high-speed fiber internet.".to_string()),
            listing_structure: "Apartment".to_string(),
            country: "Jamaica".to_string(),
            city: Some("Kingston".to_string()),
            price_per_night: Some(dec!(310.00)),
            weekly_discount_percentage: Some(dec!(5.0)),
            monthly_discount_percentage: Some(dec!(15.0)),
            is_active: true,
            added_at: Utc::now(),
            owner_name: Some("Pavel & Partners".to_string()),
            primary_image_url: Some("https://images.unsplash.com/photo-1502672260266-1c1ef2d93688?auto=format&fit=crop&w=1200&q=80".to_string()),
            max_guests: 4,
            bedrooms: 2,
            beds: 2,
            full_bathrooms: 2,
            half_bathrooms: 1,
            square_meters: Some(180),
            latitude: Some(18.0064),
            longitude: Some(-76.7891),
            overall_rating: Some(4.85),
            base_currency: "USD".to_string(),
            slug: "kingston-skyline-luxury-penthouse".to_string(),
            listing_details: None,
            minimum_stay: 1,
            days_between_bookings: 1,
        },
    ]
}

pub fn get_sample_listing_details(id_or_slug: &str) -> Option<ListingDetails> {
    let listings = get_sample_listings();
    let normalized = id_or_slug.to_lowercase();
    let listing = listings.into_iter().find(|l| {
        l.slug.to_lowercase() == normalized
            || l.id.to_string().to_lowercase() == normalized
            || normalized.contains(&l.slug.to_lowercase())
            || l.slug.to_lowercase().contains(&normalized)
    }).or_else(|| get_sample_listings().into_iter().next())?;

    let images = vec![
        ListingImageResponse {
            id: Uuid::now_v7(),
            url: listing.primary_image_url.clone().unwrap_or_default(),
        },
        ListingImageResponse {
            id: Uuid::now_v7(),
            url: "https://images.unsplash.com/photo-1600585154340-be6161a56a0c?auto=format&fit=crop&w=1200&q=80".to_string(),
        },
        ListingImageResponse {
            id: Uuid::now_v7(),
            url: "https://images.unsplash.com/photo-1540555700478-4be289fbecef?auto=format&fit=crop&w=1200&q=80".to_string(),
        },
    ];

    let mut distribution = HashMap::new();
    distribution.insert(5, 24);
    distribution.insert(4, 4);

    let rating_summary = ListingRatingSummary {
        overall_rating: listing.overall_rating,
        cleanliness_rating: Some(4.9),
        accuracy_rating: Some(5.0),
        location_rating: Some(4.9),
        value_rating: Some(4.9),
        review_count: 28,
        rating_distribution: distribution,
    };

    Some(ListingDetails {
        listing,
        images,
        host_name: Some("Pavel & Partners (Superhost)".to_string()),
        rating_summary: Some(rating_summary),
    })
}

pub fn get_sample_reviews() -> Vec<ReviewResponse> {
    vec![
        ReviewResponse {
            id: Uuid::now_v7(),
            guest_first_name: "Marcus".to_string(),
            cleanliness_rating: 5,
            accuracy_rating: 5,
            location_rating: 5,
            value_rating: 5,
            overall_rating: 5.0,
            public_review_text: Some("An absolute paradise. The views, hospitality, and comfort exceeded all expectations. We will be back next winter!".to_string()),
            host_reply_text: Some("Thank you Marcus! It was a delight hosting you and your family.".to_string()),
            host_replied_at: None,
            created_at: Utc::now(),
        },
        ReviewResponse {
            id: Uuid::now_v7(),
            guest_first_name: "Elena".to_string(),
            cleanliness_rating: 5,
            accuracy_rating: 5,
            location_rating: 4,
            value_rating: 5,
            overall_rating: 4.75,
            public_review_text: Some("Stunning property! The private chef prepared incredible Jamaican dishes every evening.".to_string()),
            host_reply_text: None,
            host_replied_at: None,
            created_at: Utc::now(),
        },
    ]
}
