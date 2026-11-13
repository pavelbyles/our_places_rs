//! Staff and services on the listing studio form.

use serde_json::json;
use uuid::Uuid;
use web_ui_admin::pages::listings::form::ListingFields;
use web_ui_admin::pages::listings::services_field::{
    INCLUDED_FIELD, NOTE_FIELD, addon_key, services_fields,
};

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
fn included_lines_note_and_addons_save() {
    let f = fields(&[
        (INCLUDED_FIELD.into(), " Private chef \n\nButler"),
        (NOTE_FIELD.into(), "Gratuity customary"),
        (addon_key(1, "name"), "Airport transfer"),
        (addon_key(1, "price"), "1,180.50"),
        (addon_key(1, "unit"), "each way"),
        (addon_key(3, "name"), "Groceries"),
    ]);
    let details = saved(&f, None);
    assert_eq!(
        details["staff_services"],
        json!({
            "included": ["Private chef", "Butler"],
            "included_note": "Gratuity customary",
            "addons": [
                { "name": "Airport transfer", "price": "1180.50", "unit": "each way" },
                { "name": "Groceries", "price": null, "unit": null },
            ],
        })
    );
}

#[test]
fn bad_prices_and_nameless_addons_name_the_row() {
    let errors = fields(&[
        (addon_key(1, "name"), "Transfer"),
        (addon_key(1, "price"), "12.345"),
        (addon_key(2, "price"), "50"),
        (addon_key(4, "name"), "Driver"),
        (addon_key(4, "price"), "-5"),
    ])
    .validate(None, false)
    .err()
    .unwrap_or_default();
    assert!(
        errors.iter().any(|e| e.starts_with("Add-on 1's price")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e == "Add-on 2 needs a name."),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.starts_with("Add-on 4's price")),
        "{errors:?}"
    );
}

#[test]
fn stored_services_fill_the_form_and_clearing_removes_them() {
    let current = json!({
        "staff_services": { "included": ["Chef"], "addons": [{ "name": "Spa", "price": "140.00", "unit": "hour" }] },
        "wifi_password": "s3cret",
    });
    let slots = services_fields(Some(&current));
    assert_eq!(slots.get(INCLUDED_FIELD).map(String::as_str), Some("Chef"));
    assert_eq!(slots.get("addon_1_price").map(String::as_str), Some("140"));
    let cleared = saved(&fields(&[]), Some(&current));
    assert!(cleared.get("staff_services").is_none(), "{cleared}");
    assert_eq!(cleared["wifi_password"], "s3cret");
}
