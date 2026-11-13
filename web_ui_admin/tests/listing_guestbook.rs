//! Guestbook quotes on the listing studio form.

use serde_json::json;
use uuid::Uuid;
use web_ui_admin::pages::listings::form::ListingFields;
use web_ui_admin::pages::listings::guestbook_field::{guestbook_fields, quote_key};

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

#[test]
fn quotes_save_and_fill_the_form_again() {
    let f = fields(&[
        (
            quote_key(2, "text"),
            " The garden was our favourite place. ",
        ),
        (quote_key(2, "name"), "Danielle"),
        (quote_key(2, "location"), "Toronto"),
        (quote_key(2, "date"), "March 2026"),
        (quote_key(2, "rating"), "5"),
    ]);
    let details = f
        .validate(None, false)
        .map(|v| v.into_new())
        .map_err(|e| e.join("; "))
        .ok()
        .and_then(|r| r.listing_details)
        .unwrap_or_default();
    assert_eq!(
        details["curated_testimonials"],
        json!([{
            "guest_name": "Danielle", "location": "Toronto", "stay_date": "March 2026",
            "quote": "The garden was our favourite place.", "rating": 5
        }])
    );
    let slots = guestbook_fields(Some(&details));
    assert_eq!(
        slots.get("quote_1_name").map(String::as_str),
        Some("Danielle")
    );
}

#[test]
fn ratings_outside_one_to_five_and_missing_credits_name_the_quote() {
    let errors = fields(&[
        (quote_key(1, "text"), "Lovely"),
        (quote_key(1, "name"), "Marcus"),
        (quote_key(1, "date"), "Dec 2025"),
        (quote_key(1, "rating"), "6"),
        (quote_key(3, "text"), "Quiet"),
        (quote_key(4, "name"), "Ana"),
    ])
    .validate(None, false)
    .err()
    .unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|e| e == "Guestbook quote 1's rating must be 1 to 5."),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("Guestbook quote 3 needs the guest's")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e == "Guestbook quote 4 needs the quote itself."),
        "{errors:?}"
    );
}
