//! Amenities on the listing studio form.

use serde_json::json;
use uuid::Uuid;
use web_ui_admin::pages::listings::form::{
    ListingFields, MAX_AMENITIES, MAX_AMENITY_CHARS, amenities_of,
};

fn fields(amenities: &str) -> ListingFields {
    ListingFields {
        name: "Sea Grape Villa".into(),
        price_per_night: "450".into(),
        amenities: amenities.into(),
        ..ListingFields::blank(Uuid::now_v7())
    }
}

#[test]
fn one_amenity_per_line_trimmed_without_blanks_or_duplicates() {
    let valid = fields("  Private pool \n\nWi-Fi\nprivate POOL\n Air conditioning")
        .validate(None, false)
        .map_err(|e| e.join("; "));
    let request = valid.map(|v| v.into_new());
    let details = request.ok().and_then(|r| r.listing_details);
    assert_eq!(
        amenities_of(details.as_ref()),
        ["Private pool", "Wi-Fi", "Air conditioning"]
    );
    assert_eq!(
        details.and_then(|d| d.get("is_active").cloned()),
        Some(json!(true))
    );
}

#[test]
fn rejects_too_many_or_too_long_amenities() {
    let many = (0..=MAX_AMENITIES)
        .map(|i| format!("Amenity {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let errors = fields(&many)
        .validate(None, false)
        .err()
        .unwrap_or_default();
    assert!(
        errors.iter().any(|e| e.contains("at most 50 amenities")),
        "{errors:?}"
    );

    let long = "x".repeat(MAX_AMENITY_CHARS + 1);
    let errors = fields(&long)
        .validate(None, false)
        .err()
        .unwrap_or_default();
    assert!(
        errors.iter().any(|e| e.contains("at most 60 characters")),
        "{errors:?}"
    );
}

#[test]
fn an_edit_keeps_the_other_listing_details() {
    let current = json!({
        "wifi_ssid": "SeaGrape",
        "wifi_password": "s3cret",
        "check_in_instructions": "Key in the lockbox",
        "amenities": ["Old amenity"],
    });
    let request = fields("Private pool\nWi-Fi")
        .validate(None, false)
        .map(|v| v.into_update(Some(&current)))
        .ok();
    let details = request.and_then(|r| r.listing_details).unwrap_or_default();
    assert_eq!(details["wifi_ssid"], "SeaGrape");
    assert_eq!(details["wifi_password"], "s3cret");
    assert_eq!(details["check_in_instructions"], "Key in the lockbox");
    assert_eq!(details["amenities"], json!(["Private pool", "Wi-Fi"]));
}

#[test]
fn clearing_the_box_removes_every_amenity() {
    let current = json!({ "amenities": ["Private pool"], "address": "1 Bay Rd" });
    let details = fields("   \n")
        .validate(None, false)
        .map(|v| v.into_update(Some(&current)))
        .ok()
        .and_then(|r| r.listing_details)
        .unwrap_or_default();
    assert_eq!(details["amenities"], json!([]));
    assert_eq!(details["address"], "1 Bay Rd");
}

#[test]
fn an_edit_of_a_listing_without_details_starts_a_fresh_object() {
    let details = fields("Wi-Fi")
        .validate(None, false)
        .map(|v| v.into_update(None))
        .ok()
        .and_then(|r| r.listing_details);
    assert_eq!(details, Some(json!({ "amenities": ["Wi-Fi"] })));
}
