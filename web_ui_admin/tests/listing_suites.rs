//! Suites on the listing studio form.

use serde_json::json;
use uuid::Uuid;
use web_ui_admin::pages::listings::form::ListingFields;
use web_ui_admin::pages::listings::suite_field::{parse_suites, suite_fields, suite_key};

fn fields(suites: &[(usize, &[(&str, &str)])]) -> ListingFields {
    let mut f = ListingFields {
        name: "Sea Grape Villa".into(),
        price_per_night: "450".into(),
        ..ListingFields::blank(Uuid::now_v7())
    };
    for (n, parts) in suites {
        for (part, value) in *parts {
            f.slots.insert(suite_key(*n, part), value.to_string());
        }
    }
    f
}

#[test]
fn filled_slots_save_as_suites_in_slot_order() {
    let photo = Uuid::now_v7();
    let photo_id = photo.to_string();
    let f = fields(&[
        (
            2,
            &[
                ("name", "Garden Room"),
                ("beds", "1 queen"),
                ("guests", "2"),
            ],
        ),
        (
            1,
            &[
                ("name", " Oceanfront Master "),
                ("beds", "1 king"),
                ("guests", "2"),
                ("bath", "En-suite, soaking tub"),
                ("features", "Ocean view,  Private balcony , "),
                ("image", photo_id.as_str()),
            ],
        ),
    ]);
    let details = f
        .validate(None, false)
        .map(|v| v.into_new())
        .map_err(|e| e.join("; "))
        .ok()
        .and_then(|r| r.listing_details)
        .unwrap_or_default();
    assert_eq!(details["suites"][0]["name"], "Oceanfront Master");
    assert_eq!(
        details["suites"][0]["features"],
        json!(["Ocean view", "Private balcony"])
    );
    assert_eq!(details["suites"][0]["image_id"], json!(photo));
    assert_eq!(details["suites"][1]["name"], "Garden Room");
    assert!(details["suites"][1]["en_suite"].is_null(), "{details}");
}

#[test]
fn half_filled_and_bad_values_name_the_suite() {
    let errors = fields(&[
        (1, &[("name", "Master"), ("beds", ""), ("guests", "2")]),
        (3, &[("name", "Loft"), ("beds", "2 twins"), ("guests", "0")]),
        (
            4,
            &[
                ("name", "Den"),
                ("beds", "1 sofa bed"),
                ("guests", "1"),
                ("image", "not-a-photo"),
            ],
        ),
    ])
    .validate(None, false)
    .err()
    .unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|e| e == "Suite 1 needs a name and a bed setup."),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.starts_with("Suite 3's guests")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e == "Choose a photo from the list for suite 4."),
        "{errors:?}"
    );
}

#[test]
fn stored_suites_fill_the_slots_and_clearing_removes_them() {
    let current = json!({
        "suites": [{ "name": "Master", "bed_configuration": "1 king", "max_guests": 2, "features": ["Ocean view", "Balcony"] }],
        "wifi_password": "s3cret",
    });
    let slots = suite_fields(Some(&current));
    assert_eq!(
        slots.get("suite_1_features").map(String::as_str),
        Some("Ocean view, Balcony")
    );
    assert_eq!(slots.get("suite_1_guests").map(String::as_str), Some("2"));
    assert_eq!(parse_suites(&slots).map(|s| s.len()), Ok(1));

    let cleared = fields(&[])
        .validate(None, false)
        .map(|v| v.into_update(Some(&current)))
        .ok()
        .and_then(|r| r.listing_details)
        .unwrap_or_default();
    assert!(cleared.get("suites").is_none(), "{cleared}");
    assert_eq!(cleared["wifi_password"], "s3cret");
}
