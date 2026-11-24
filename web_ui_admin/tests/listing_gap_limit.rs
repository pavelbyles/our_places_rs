//! The studio form's limit on days between bookings.

use uuid::Uuid;
use web_ui_admin::pages::listings::form::ListingFields;

fn errors(gap: &str) -> Vec<String> {
    ListingFields {
        name: "Sea Grape Villa".into(),
        price_per_night: "450".into(),
        days_between_bookings: gap.into(),
        ..ListingFields::blank(Uuid::now_v7())
    }
    .validate(None, false)
    .err()
    .unwrap_or_default()
}

fn mentions_the_gap(errors: &[String]) -> bool {
    errors.iter().any(|e| e.contains("Days between bookings"))
}

#[test]
fn accepts_gaps_within_the_limit() {
    for gap in ["0", "365"] {
        let found = errors(gap);
        assert!(!mentions_the_gap(&found), "{gap}: {found:?}");
    }
}

#[test]
fn refuses_gaps_outside_the_limit() {
    for gap in ["-1", "366", "2147483647", "abc"] {
        let found = errors(gap);
        assert!(
            found
                .iter()
                .any(|e| e == "Days between bookings must be a whole number from 0 to 365."),
            "{gap}: {found:?}"
        );
    }
}
