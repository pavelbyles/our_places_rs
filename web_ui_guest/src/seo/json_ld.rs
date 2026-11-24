//! The listing page's schema.org `VacationRental` (spec 102 §1.4), as typed
//! structs serialized to JSON-LD. Fields with no value are left out, never
//! `null`. Only public data is used: the coordinates `listing_api` sends on
//! public paths (approximate when the host chose so) and no street address.

use std::collections::HashSet;

use common::listing_details::amenities::MAX_AMENITIES;
use common::listing_details::{StructuredListingDetails, amenity_groups_for};
use common::models::{ListingDetails, ListingRatingSummary, ListingResponse};
use serde::Serialize;

use super::truncate_on_word;

/// Longest description, in characters.
const DESCRIPTION_CHARS: usize = 5000;

/// A whole property, let as one unit.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VacationRental {
    #[serde(rename = "@context")]
    context: &'static str,
    #[serde(rename = "@type")]
    kind: &'static str,
    #[serde(rename = "@id")]
    id: String,
    identifier: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    url: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    image: Vec<String>,
    address: PostalAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    longitude: Option<f64>,
    contains_place: Accommodation,
    #[serde(skip_serializing_if = "Option::is_none")]
    aggregate_rating: Option<AggregateRating>,
    // `offers` is not a property of LodgingBusiness / VacationRental;
    // `makesOffer` (from Organization) is.
    #[serde(skip_serializing_if = "Option::is_none")]
    makes_offer: Option<Offer>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PostalAddress {
    #[serde(rename = "@type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    address_locality: Option<String>,
    address_country: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Accommodation {
    #[serde(rename = "@type")]
    kind: &'static str,
    additional_type: &'static str,
    occupancy: QuantitativeValue,
    number_of_bedrooms: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    number_of_beds: Option<i32>,
    number_of_full_bathrooms: i32,
    number_of_partial_bathrooms: i32,
    number_of_bathrooms_total: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    floor_size: Option<QuantitativeValue>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    amenity_feature: Vec<LocationFeatureSpecification>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuantitativeValue {
    #[serde(rename = "@type")]
    kind: &'static str,
    value: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_code: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct LocationFeatureSpecification {
    #[serde(rename = "@type")]
    kind: &'static str,
    name: String,
    value: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AggregateRating {
    #[serde(rename = "@type")]
    kind: &'static str,
    /// A review score (not money), rounded to two decimals.
    rating_value: f64,
    review_count: i32,
    best_rating: i32,
    worst_rating: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Offer {
    #[serde(rename = "@type")]
    kind: &'static str,
    url: String,
    price_specification: UnitPriceSpecification,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnitPriceSpecification {
    #[serde(rename = "@type")]
    kind: &'static str,
    /// The `Decimal` as written, never through a float.
    price: String,
    price_currency: String,
    unit_code: &'static str,
    unit_text: &'static str,
}

fn quantity(value: i32, unit_code: Option<&'static str>) -> QuantitativeValue {
    QuantitativeValue {
        kind: "QuantitativeValue",
        value,
        unit_code,
    }
}

/// The host's amenities, trimmed, deduplicated in order, at most
/// [`MAX_AMENITIES`].
fn amenities(listing: &ListingResponse) -> Vec<LocationFeatureSpecification> {
    let structured = StructuredListingDetails::from_json(listing.listing_details.as_ref());
    let mut seen = HashSet::new();
    amenity_groups_for(structured.amenity_groups.as_deref(), &structured.amenities)
        .into_iter()
        .flat_map(|group| group.items)
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty() && seen.insert(item.clone()))
        .take(MAX_AMENITIES)
        .map(|name| LocationFeatureSpecification {
            kind: "LocationFeatureSpecification",
            name,
            value: true,
        })
        .collect()
}

fn rating(summary: Option<&ListingRatingSummary>) -> Option<AggregateRating> {
    summary
        .filter(|s| s.review_count > 0)
        .and_then(|s| s.overall_rating.map(|r| (r, s.review_count)))
        .filter(|(r, _)| r.is_finite())
        .map(|(r, count)| AggregateRating {
            kind: "AggregateRating",
            rating_value: (r * 100.0).round() / 100.0,
            review_count: count,
            best_rating: 5,
            worst_rating: 1,
        })
}

fn offer(listing: &ListingResponse, canonical: &str) -> Option<Offer> {
    listing.price_per_night.map(|price| Offer {
        kind: "Offer",
        url: canonical.to_string(),
        price_specification: UnitPriceSpecification {
            kind: "UnitPriceSpecification",
            price: price.to_string(),
            price_currency: listing.base_currency.clone(),
            unit_code: "DAY",
            unit_text: "night",
        },
    })
}

/// The listing as a `VacationRental`, identified by its `canonical` URL.
/// The price is the one the page shows: the base nightly rate in the base
/// currency, or the converted pair when the visitor chose a currency (spec
/// §1.4.5).
pub fn vacation_rental(details: &ListingDetails, canonical: &str) -> VacationRental {
    let listing = &details.listing;
    let description = listing
        .description
        .as_deref()
        .map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|d| !d.is_empty())
        .map(|d| truncate_on_word(&d, DESCRIPTION_CHARS));
    VacationRental {
        context: "https://schema.org",
        kind: "VacationRental",
        id: format!("{canonical}#rental"),
        identifier: listing.id.to_string(),
        name: listing.name.clone(),
        description,
        url: canonical.to_string(),
        image: details.images.iter().map(|i| i.url.clone()).collect(),
        address: PostalAddress {
            kind: "PostalAddress",
            address_locality: listing
                .city
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .map(str::to_string),
            address_country: listing.country.clone(),
        },
        latitude: listing.latitude,
        longitude: listing.longitude,
        contains_place: Accommodation {
            kind: "Accommodation",
            additional_type: "EntirePlace",
            occupancy: quantity(listing.max_guests.max(1), None),
            number_of_bedrooms: listing.bedrooms,
            // 0 means the host never entered a bed count.
            number_of_beds: (listing.beds > 0).then_some(listing.beds),
            number_of_full_bathrooms: listing.full_bathrooms,
            number_of_partial_bathrooms: listing.half_bathrooms,
            number_of_bathrooms_total: listing.full_bathrooms + listing.half_bathrooms,
            floor_size: listing.square_meters.map(|m| quantity(m, Some("MTK"))),
            amenity_feature: amenities(listing),
        },
        aggregate_rating: rating(details.rating_summary.as_ref()),
        makes_offer: offer(listing, canonical),
    }
}
