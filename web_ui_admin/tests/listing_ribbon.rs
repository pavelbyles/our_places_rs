//! The "Highlights ribbon" rows on the listing studio form.

use serde_json::json;
use uuid::Uuid;
use web_ui_admin::pages::listings::form::ListingFields;
use web_ui_admin::pages::listings::ribbon_field::{parse_ribbon, ribbon_fields, ribbon_key};

/// The form as a browser posts it.
fn posted(rows: &[(usize, &str, &str, &str)]) -> ListingFields {
    let mut body = vec![
        ("name".to_string(), "Sea Grape Villa".to_string()),
        ("price_per_night".to_string(), "450".to_string()),
        ("listing_structure".to_string(), "Villa".to_string()),
        ("country".to_string(), "Jamaica".to_string()),
        ("base_currency".to_string(), "USD".to_string()),
        ("max_guests".to_string(), "6".to_string()),
        ("guests_included".to_string(), "2".to_string()),
        ("bedrooms".to_string(), "3".to_string()),
        ("beds".to_string(), "3".to_string()),
        ("full_bathrooms".to_string(), "2".to_string()),
        ("half_bathrooms".to_string(), "0".to_string()),
        ("minimum_stay".to_string(), "1".to_string()),
        ("days_between_bookings".to_string(), "0".to_string()),
        ("owner_id".to_string(), Uuid::now_v7().to_string()),
    ];
    for n in 1..=6 {
        let (icon, value, label) = rows
            .iter()
            .find(|r| r.0 == n)
            .map(|r| (r.1, r.2, r.3))
            .unwrap_or(("bed", "", ""));
        body.push((ribbon_key(n, "icon"), icon.into()));
        body.push((ribbon_key(n, "value"), value.into()));
        body.push((ribbon_key(n, "label"), label.into()));
    }
    let encoded = serde_urlencoded::to_string(&body).unwrap_or_default();
    match serde_urlencoded::from_str::<ListingFields>(&encoded) {
        Ok(f) => f,
        Err(e) => panic!("form body did not parse: {e}"),
    }
}

#[test]
fn rows_post_into_the_ribbon_in_row_order_skipping_empty_rows() {
    let f = posted(&[
        (1, "bed", "3", "Bedrooms"),
        (3, "beach", " Direct ", "Beach access"),
        (6, "pool", "1", "Private pool"),
    ]);
    let request = f
        .validate(None, false)
        .map(|v| v.into_new())
        .map_err(|e| e.join("; "));
    let details = request
        .ok()
        .and_then(|r| r.listing_details)
        .unwrap_or_default();
    assert_eq!(
        details["ribbon"],
        json!([
            { "icon": "bed", "value": "3", "label": "Bedrooms" },
            { "icon": "beach", "value": "Direct", "label": "Beach access" },
            { "icon": "pool", "value": "1", "label": "Private pool" },
        ])
    );
}

#[test]
fn a_half_filled_row_names_the_row_and_keeps_the_input() {
    let f = posted(&[
        (1, "bed", "3", "Bedrooms"),
        (4, "bath", "2", ""),
        (5, "star", "x", "y"),
    ]);
    let errors = f.validate(None, false).err().unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|e| e == "Ribbon item 4 needs a value and a label."),
        "{errors:?}"
    );
    assert_eq!(f.slots.get("ribbon_4_value").map(String::as_str), Some("2"));
}

#[test]
fn too_few_items_are_refused_but_none_at_all_is_fine() {
    let errors = posted(&[(1, "bed", "3", "Bedrooms"), (2, "bath", "2", "Bathrooms")])
        .validate(None, false)
        .err()
        .unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("Fill in 3 to 6 ribbon items")),
        "{errors:?}"
    );
    assert_eq!(parse_ribbon(&posted(&[]).slots), Ok(None));
}

#[test]
fn too_long_values_and_unknown_icons_are_refused() {
    let errors = posted(&[(2, "rocket", "123456789", "Bedrooms")])
        .validate(None, false)
        .err()
        .unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("item 2's value can be at most 8")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e == "Choose an icon for ribbon item 2."),
        "{errors:?}"
    );
}

#[test]
fn saving_keeps_other_details_and_clearing_removes_the_ribbon() {
    let current = json!({
        "amenities": ["Pool"],
        "wifi_password": "s3cret",
        "ribbon": [{ "icon": "bed", "value": "9", "label": "Old" }],
    });
    let filled = posted(&[
        (1, "bed", "3", "Bedrooms"),
        (2, "bath", "2", "Bathrooms"),
        (3, "guests", "6", "Guests"),
    ]);
    let details = filled
        .validate(None, false)
        .map(|v| v.into_update(Some(&current)))
        .ok()
        .and_then(|r| r.listing_details)
        .unwrap_or_default();
    assert_eq!(details["wifi_password"], "s3cret");
    assert_eq!(details["ribbon"][0]["value"], "3");

    let cleared = posted(&[])
        .validate(None, false)
        .map(|v| v.into_update(Some(&current)))
        .ok()
        .and_then(|r| r.listing_details)
        .unwrap_or_default();
    assert!(cleared.get("ribbon").is_none(), "{cleared}");
    assert_eq!(cleared["wifi_password"], "s3cret");
}

#[test]
fn an_existing_ribbon_fills_the_rows() {
    let details = json!({ "ribbon": [
        { "icon": "beach", "value": "Direct", "label": "Beach access" },
        { "icon": "pool", "value": "1", "label": "Pool" },
        { "icon": "bed", "value": "6", "label": "Bedrooms" },
    ]});
    let fields = ribbon_fields(Some(&details));
    assert_eq!(
        fields.get("ribbon_1_icon").map(String::as_str),
        Some("beach")
    );
    assert_eq!(
        fields.get("ribbon_3_label").map(String::as_str),
        Some("Bedrooms")
    );
    assert!(!fields.contains_key("ribbon_4_value"));
    assert!(ribbon_fields(None).is_empty());
}
