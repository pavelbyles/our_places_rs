//! Amenity groups on the listing studio form.

use serde_json::json;
use uuid::Uuid;
use web_ui_admin::pages::listings::amenity_field::{amenity_fields, amenity_groups_of, group_key};
use web_ui_admin::pages::listings::form::{ListingFields, MAX_AMENITIES, MAX_AMENITY_CHARS};

/// A new-listing form with `groups` as (slot, title, icon, items).
fn fields(groups: &[(usize, &str, &str, &str)]) -> ListingFields {
    let mut f = ListingFields {
        name: "Sea Grape Villa".into(),
        price_per_night: "450".into(),
        ..ListingFields::blank(Uuid::now_v7())
    };
    for (n, title, icon, items) in groups {
        f.slots.insert(group_key(*n, "title"), title.to_string());
        f.slots.insert(group_key(*n, "icon"), icon.to_string());
        f.slots.insert(group_key(*n, "items"), items.to_string());
    }
    f
}

fn saved_groups(f: &ListingFields, current: Option<&serde_json::Value>) -> serde_json::Value {
    f.validate(None, false)
        .map(|v| v.into_update(current))
        .ok()
        .and_then(|r| r.listing_details)
        .unwrap_or_default()
}

#[test]
fn a_new_listing_starts_with_the_suggested_group_titles() {
    let f = ListingFields::blank(Uuid::now_v7());
    let titles: Vec<_> = (1..=4)
        .filter_map(|n| f.slots.get(&group_key(n, "title")).cloned())
        .collect();
    assert_eq!(
        titles,
        ["Villa", "Health & Wellness", "Family", "Common Areas"]
    );
    assert!(!f.slots.contains_key(&group_key(5, "title")));
}

#[test]
fn suggested_titles_without_items_are_not_saved() {
    let details = fields(&[(2, "Health & Wellness", "waves", "Infinity pool")])
        .validate(None, false)
        .map(|v| v.into_new())
        .ok()
        .and_then(|r| r.listing_details)
        .unwrap_or_default();
    assert_eq!(
        details["amenity_groups"],
        json!([{ "title": "Health & Wellness", "icon": "waves", "items": ["Infinity pool"] }])
    );
}

#[test]
fn one_amenity_per_line_trimmed_without_blanks_or_duplicates() {
    let f = fields(&[(
        1,
        "Villa",
        "home",
        "  Private pool \n\nWi-Fi\nprivate POOL\n Air conditioning",
    )]);
    let groups = amenity_groups_of(Some(&saved_groups(&f, None)));
    assert_eq!(
        groups[0].items,
        ["Private pool", "Wi-Fi", "Air conditioning"]
    );
}

#[test]
fn a_group_with_items_needs_a_title() {
    let errors = fields(&[(3, "  ", "sun", "Gazebo")])
        .validate(None, false)
        .err()
        .unwrap_or_default();
    assert!(
        errors.iter().any(|e| e == "Amenity group 3 needs a title."),
        "{errors:?}"
    );
}

#[test]
fn rejects_too_many_or_too_long_amenities() {
    let half = |from: usize| {
        (from..from + MAX_AMENITIES / 2 + 1)
            .map(|i| format!("Amenity {i}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let errors = fields(&[
        (1, "Villa", "home", &half(0)),
        (2, "More", "star", &half(100)),
    ])
    .validate(None, false)
    .err()
    .unwrap_or_default();
    assert!(
        errors.iter().any(|e| e.contains("at most 20 items")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.contains("at most 50 amenities")),
        "{errors:?}"
    );

    let long = "x".repeat(MAX_AMENITY_CHARS + 1);
    let errors = fields(&[(1, "Villa", "home", &long)])
        .validate(None, false)
        .err()
        .unwrap_or_default();
    assert!(
        errors.iter().any(|e| e.contains("at most 60 characters")),
        "{errors:?}"
    );
}

#[test]
fn an_edit_keeps_the_other_listing_details_and_moves_a_legacy_list_into_groups() {
    let current = json!({
        "wifi_ssid": "SeaGrape",
        "check_in_instructions": "Key in the lockbox",
        "amenities": ["Old amenity"],
    });
    let mut f = fields(&[]);
    f.slots = amenity_fields(Some(&current));
    assert_eq!(
        f.slots.get("group_1_title").map(String::as_str),
        Some("Amenities")
    );
    assert_eq!(
        f.slots.get("group_1_items").map(String::as_str),
        Some("Old amenity")
    );

    f.slots
        .insert(group_key(1, "items"), "Old amenity\nWi-Fi".into());
    let details = saved_groups(&f, Some(&current));
    assert_eq!(details["wifi_ssid"], "SeaGrape");
    assert_eq!(details["check_in_instructions"], "Key in the lockbox");
    assert!(
        details.get("amenities").is_none(),
        "legacy key removed: {details}"
    );
    assert_eq!(
        details["amenity_groups"][0]["items"],
        json!(["Old amenity", "Wi-Fi"])
    );
}

#[test]
fn clearing_every_group_removes_the_amenities() {
    let current = json!({
        "amenity_groups": [{ "title": "Villa", "icon": "home", "items": ["Pool"] }],
        "address": "1 Bay Rd",
    });
    let details = saved_groups(&fields(&[(1, "Villa", "home", "   \n")]), Some(&current));
    assert!(details.get("amenity_groups").is_none(), "{details}");
    assert_eq!(details["address"], "1 Bay Rd");
}

#[test]
fn an_edit_of_a_listing_without_details_starts_a_fresh_object() {
    let details = fields(&[(1, "Villa", "home", "Wi-Fi")])
        .validate(None, false)
        .map(|v| v.into_update(None))
        .ok()
        .and_then(|r| r.listing_details);
    assert_eq!(
        details,
        Some(
            json!({ "amenity_groups": [{ "title": "Villa", "icon": "home", "items": ["Wi-Fi"] }] })
        )
    );
}
