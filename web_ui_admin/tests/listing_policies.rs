//! House policies and nearby places on the listing studio form.

use serde_json::json;
use uuid::Uuid;
use web_ui_admin::pages::listings::form::ListingFields;
use web_ui_admin::pages::listings::policies_field::{nearby_key, policies_fields};

fn fields(pairs: &[(String, &str)]) -> ListingFields {
    let mut f = ListingFields {
        name: "Sea Grape Villa".into(),
        price_per_night: "450".into(),
        ..ListingFields::blank(Uuid::now_v7())
    };
    for (k, v) in pairs {
        f.slots.insert(k.clone(), v.to_string());
    }
    f
}

fn saved(f: &ListingFields, current: Option<&serde_json::Value>) -> serde_json::Value {
    f.validate(None, false)
        .map(|v| v.into_update(current))
        .map_err(|e| e.join("; "))
        .ok()
        .and_then(|r| r.listing_details)
        .unwrap_or_default()
}

#[test]
fn policies_and_nearby_places_save_and_keep_other_details() {
    let current = json!({ "wifi_password": "s3cret" });
    let f = fields(&[
        ("policy_check_in".into(), " From 3:00 PM "),
        ("policy_house_rules".into(), "No smoking indoors"),
        (nearby_key(1, "name"), "Green Grotto Caves"),
        (nearby_key(1, "minutes"), "10"),
        (nearby_key(3, "name"), "Sangster Intl. (MBJ)"),
        (nearby_key(3, "minutes"), "75"),
    ]);
    let details = saved(&f, Some(&current));
    assert_eq!(details["policies"]["check_in"], "From 3:00 PM");
    assert!(details["policies"]["check_out"].is_null());
    assert_eq!(
        details["nearby"],
        json!([
            { "name": "Green Grotto Caves", "drive_minutes": 10 },
            { "name": "Sangster Intl. (MBJ)", "drive_minutes": 75 },
        ])
    );
    assert_eq!(details["wifi_password"], "s3cret");
    let refill = policies_fields(Some(&details));
    assert_eq!(
        refill.get("nearby_2_minutes").map(String::as_str),
        Some("75")
    );
    assert_eq!(
        refill.get("policy_check_in").map(String::as_str),
        Some("From 3:00 PM")
    );
}

#[test]
fn bad_rows_name_the_place_and_long_policies_name_the_field() {
    let long = "x".repeat(161);
    let errors = fields(&[
        ("policy_cancellation".into(), long.as_str()),
        (nearby_key(2, "name"), "Port Royal"),
        (nearby_key(2, "minutes"), "0"),
        (nearby_key(4, "minutes"), "20"),
    ])
    .validate(None, false)
    .err()
    .unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|e| e == "Cancellation can be at most 160 characters."),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("Nearby place 2's drive time")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("Nearby place 4 needs a name")),
        "{errors:?}"
    );
}

#[test]
fn clearing_removes_policies_and_nearby_places() {
    let current = json!({
        "policies": { "check_in": "From 3" },
        "nearby": [{ "name": "X", "drive_minutes": 5 }],
    });
    let details = saved(&fields(&[]), Some(&current));
    assert!(
        details.get("policies").is_none() && details.get("nearby").is_none(),
        "{details}"
    );
}
