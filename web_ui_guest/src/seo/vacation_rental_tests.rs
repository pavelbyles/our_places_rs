//! Spec 102 §1.4 (U6, U7): the listing's `VacationRental` JSON-LD.

use common::models::ListingDetails;
use serde_json::{Value, json};

use super::json_ld::vacation_rental;

const CANONICAL: &str = "https://ourplaces.io/listings/villa-azure";

/// A complete listing; tests change one thing at a time.
fn details() -> Value {
    json!({
        "listing": {
            "id": "0190a0a0-0000-7000-8000-000000000001",
            "user_id": "0190a0a0-0000-7000-8000-000000000002",
            "name": "Villa Azure",
            "description": "  Sea views,\n private pool. ",
            "listing_structure": "Villa",
            "country": "Jamaica",
            "city": "Negril",
            "is_active": true,
            "added_at": "2026-01-01T00:00:00Z",
            "max_guests": 6, "bedrooms": 3, "beds": 4,
            "full_bathrooms": 2, "half_bathrooms": 1,
            "square_meters": 280,
            "latitude": 18.27, "longitude": -78.35,
            "price_per_night": "450.00",
            "base_currency": "USD", "slug": "villa-azure",
            "minimum_stay": 2, "days_between_bookings": 0,
            "listing_details": { "amenities": ["Pool", "Wifi", "Pool", "  ", "Chef"] }
        },
        "images": [
            { "id": "0190a0a0-0000-7000-8000-0000000000a1", "url": "https://img.example/1.webp" },
            { "id": "0190a0a0-0000-7000-8000-0000000000a2", "url": "https://img.example/2.webp" }
        ],
        "rating_summary": {
            "overall_rating": 4.666666, "review_count": 3,
            "rating_distribution": {}
        }
    })
}

fn parse(value: Value) -> ListingDetails {
    match serde_json::from_value(value) {
        Ok(d) => d,
        Err(e) => panic!("fixture: {e}"),
    }
}

fn ld(value: Value) -> Value {
    serde_json::to_value(vacation_rental(&parse(value), CANONICAL)).unwrap_or_default()
}

#[test]
fn identity_fields_follow_schema_org() {
    let out = ld(details());
    assert_eq!(out["@context"], "https://schema.org");
    assert_eq!(out["@type"], "VacationRental");
    assert_eq!(out["@id"], format!("{CANONICAL}#rental"));
    assert_eq!(out["url"], CANONICAL);
    assert_eq!(out["identifier"], "0190a0a0-0000-7000-8000-000000000001");
    assert_eq!(out["name"], "Villa Azure");
    assert_eq!(out["description"], "Sea views, private pool.");
    assert!(
        out.get("offers").is_none(),
        "offers is not valid on VacationRental"
    );
}

#[test]
fn address_has_locality_and_country_but_no_street() {
    let out = ld(details());
    assert_eq!(
        out["address"],
        json!({ "@type": "PostalAddress", "addressLocality": "Negril", "addressCountry": "Jamaica" })
    );
}

#[test]
fn coordinates_are_the_public_ones_and_optional() {
    let out = ld(details());
    assert_eq!(out["latitude"], 18.27);
    assert_eq!(out["longitude"], -78.35);
    let mut d = details();
    d["listing"]["latitude"] = Value::Null;
    d["listing"]["longitude"] = Value::Null;
    let out = ld(d);
    assert!(
        out.get("latitude").is_none() && out.get("longitude").is_none(),
        "{out}"
    );
}

#[test]
fn accommodation_describes_rooms_and_occupancy() {
    let place = &ld(details())["containsPlace"];
    assert_eq!(place["@type"], "Accommodation");
    assert_eq!(place["additionalType"], "EntirePlace");
    assert_eq!(
        place["occupancy"],
        json!({ "@type": "QuantitativeValue", "value": 6 })
    );
    assert_eq!(place["numberOfBedrooms"], 3);
    assert_eq!(place["numberOfBeds"], 4);
    assert_eq!(place["numberOfFullBathrooms"], 2);
    assert_eq!(place["numberOfPartialBathrooms"], 1);
    assert_eq!(place["numberOfBathroomsTotal"], 3);
    assert_eq!(
        place["floorSize"],
        json!({ "@type": "QuantitativeValue", "value": 280, "unitCode": "MTK" })
    );
}

#[test]
fn unknown_beds_size_and_occupancy_are_handled() {
    let mut d = details();
    d["listing"]["beds"] = json!(0);
    d["listing"]["square_meters"] = Value::Null;
    d["listing"]["max_guests"] = json!(0);
    let place = &ld(d)["containsPlace"];
    assert!(place.get("numberOfBeds").is_none(), "{place}");
    assert!(place.get("floorSize").is_none(), "{place}");
    assert_eq!(place["occupancy"]["value"], 1);
}

#[test]
fn amenities_are_trimmed_deduplicated_features() {
    let features = &ld(details())["containsPlace"]["amenityFeature"];
    let names: Vec<&str> = features
        .as_array()
        .map(|a| a.iter().filter_map(|f| f["name"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(names, ["Pool", "Wifi", "Chef"]);
    assert_eq!(
        features[0],
        json!({ "@type": "LocationFeatureSpecification", "name": "Pool", "value": true })
    );
}

#[test]
fn amenities_are_capped_at_fifty() {
    let mut d = details();
    let many: Vec<String> = (0..80).map(|i| format!("Amenity {i}")).collect();
    d["listing"]["listing_details"] = json!({ "amenities": many });
    let features = &ld(d)["containsPlace"]["amenityFeature"];
    assert_eq!(features.as_array().map(Vec::len), Some(50));
}

#[test]
fn images_are_listed_in_display_order_and_optional() {
    let out = ld(details());
    assert_eq!(
        out["image"],
        json!(["https://img.example/1.webp", "https://img.example/2.webp"])
    );
    let mut d = details();
    d["images"] = json!([]);
    assert!(ld(d).get("image").is_none());
}

#[test]
fn rating_is_rounded_and_needs_reviews() {
    let out = ld(details());
    assert_eq!(
        out["aggregateRating"],
        json!({
            "@type": "AggregateRating", "ratingValue": 4.67, "reviewCount": 3,
            "bestRating": 5, "worstRating": 1
        })
    );
    let mut d = details();
    d["rating_summary"]["review_count"] = json!(0);
    assert!(ld(d).get("aggregateRating").is_none());
    let mut d = details();
    d["rating_summary"]["overall_rating"] = Value::Null;
    assert!(ld(d).get("aggregateRating").is_none());
}

#[test]
fn a_non_finite_rating_is_left_out() {
    let mut d = parse(details());
    if let Some(summary) = d.rating_summary.as_mut() {
        summary.overall_rating = Some(f64::NAN);
    }
    let out = serde_json::to_value(vacation_rental(&d, CANONICAL)).unwrap_or_default();
    assert!(out.get("aggregateRating").is_none(), "{out}");
}

#[test]
fn the_offer_is_the_base_nightly_rate_as_an_exact_string() {
    let offer = &ld(details())["makesOffer"];
    assert_eq!(
        *offer,
        json!({
            "@type": "Offer",
            "url": CANONICAL,
            "priceSpecification": {
                "@type": "UnitPriceSpecification",
                "price": "450.00",
                "priceCurrency": "USD",
                "unitCode": "DAY",
                "unitText": "night"
            }
        })
    );
}

#[test]
fn no_price_means_no_offer() {
    let mut d = details();
    d["listing"]["price_per_night"] = Value::Null;
    assert!(ld(d).get("makesOffer").is_none());
}

#[test]
fn a_converted_price_keeps_its_currency() {
    // listing_api replaced price and currency for a display currency (U7).
    let mut d = details();
    d["listing"]["price_per_night"] = json!("412.37");
    d["listing"]["base_currency"] = json!("EUR");
    let spec = &ld(d)["makesOffer"]["priceSpecification"];
    assert_eq!(spec["price"], "412.37");
    assert_eq!(spec["priceCurrency"], "EUR");
}

#[test]
fn a_long_description_is_capped_by_characters() {
    let mut d = details();
    d["listing"]["description"] = json!("Été 🌴 ".repeat(2000));
    let out = ld(d);
    let text = out["description"].as_str().unwrap_or_default();
    assert!(text.chars().count() <= 5000, "{}", text.chars().count());
    assert!(text.ends_with('…'));
}

#[test]
fn a_blank_description_is_left_out() {
    let mut d = details();
    d["listing"]["description"] = json!("   ");
    assert!(ld(d).get("description").is_none());
}
