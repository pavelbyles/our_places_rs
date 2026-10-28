//! Tests for the stay-dates calendar (spec FR-3.3, D3/D4).

use chrono::NaiveDate;
use common::availability::UnavailableRange;
use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use web_ui_common::components::date_range_calendar::{
    CalendarProps, DayState, calendar_url, date_range_calendar, day_label, day_state, is_booked,
    month_grid,
};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap_or_default()
}

async fn html(v: impl topcoat::view::View, cx: &Cx) -> String {
    match v.first().await {
        Ok(handle) => handle.render(cx),
        Err(e) => panic!("render failed: {e}"),
    }
}

#[test]
fn leap_february_has_29_days_after_two_blank_cells() {
    // 1 Feb 2028 is a Tuesday.
    let grid = month_grid(d(2028, 2, 1));
    assert_eq!(grid.iter().flatten().count(), 29);
    assert_eq!(&grid[..3], &[None, None, Some(d(2028, 2, 1))]);
    assert_eq!(grid.len() % 7, 0);
}

#[test]
fn month_starting_on_sunday_has_no_leading_blanks() {
    // 1 Aug 2027 is a Sunday.
    let grid = month_grid(d(2027, 8, 15));
    assert_eq!(grid.first(), Some(&Some(d(2027, 8, 1))));
    assert_eq!(grid.iter().flatten().count(), 31);
}

#[test]
fn checkout_day_is_free_for_a_new_check_in() {
    let ranges = [UnavailableRange {
        date_from: d(2027, 1, 10),
        date_to: d(2027, 1, 14),
    }];
    assert!(!is_booked(d(2027, 1, 9), &ranges));
    assert!(is_booked(d(2027, 1, 10), &ranges));
    assert!(is_booked(d(2027, 1, 13), &ranges));
    assert!(!is_booked(d(2027, 1, 14), &ranges));
}

#[test]
fn past_beats_booked_and_labels_say_why() {
    let ranges = [UnavailableRange {
        date_from: d(2027, 1, 1),
        date_to: d(2027, 1, 20),
    }];
    let today = d(2027, 1, 5);
    assert_eq!(day_state(d(2027, 1, 4), today, &ranges), DayState::Past);
    assert_eq!(day_state(d(2027, 1, 5), today, &ranges), DayState::Booked);
    assert_eq!(
        day_state(d(2027, 1, 25), today, &ranges),
        DayState::Available
    );
    assert_eq!(
        day_label(d(2027, 1, 12), DayState::Booked),
        "Tuesday, 12 January 2027, booked"
    );
    assert_eq!(
        day_label(d(2027, 1, 25), DayState::Available),
        "Monday, 25 January 2027, available"
    );
}

#[test]
fn fragment_url_keeps_the_selection() {
    assert_eq!(
        calendar_url("villa-a", d(2027, 3, 1), Some(d(2027, 3, 4)), None),
        "/listings/villa-a/calendar?month=2027-03&check_in=2027-03-04"
    );
}

#[tokio::test]
async fn renders_two_months_with_booked_nights_disabled() {
    let __cx = &Cx::default();
    let props = CalendarProps {
        listing_slug: "villa-a".into(),
        month: d(2027, 1, 1),
        months: 2,
        today: d(2027, 1, 5),
        ranges: vec![UnavailableRange {
            date_from: d(2027, 1, 10),
            date_to: d(2027, 1, 14),
        }],
        check_in: Some(d(2027, 2, 2)),
        check_out: None,
    };
    let out = html(view! { date_range_calendar(props: props) }, __cx).await;
    assert_eq!(out.matches(r#"role="grid""#).count(), 2, "two months");
    assert_eq!(out.matches("data-date=").count(), 31 + 28, "{out}");
    // 4 past (1-4 Jan) + 4 booked (10-13 Jan) nights are disabled.
    assert_eq!(out.matches(r#"aria-disabled="true""#).count(), 8, "{out}");
    assert!(
        out.contains(r#"data-date="2027-01-13" data-state="booked""#),
        "{out}"
    );
    assert!(
        out.contains(r#"data-date="2027-01-14" data-state="available""#),
        "{out}"
    );
    assert_eq!(out.matches(r#"aria-selected="true""#).count(), 1, "{out}");
    assert!(
        out.contains(r#"value="2027-02-02""#) && out.contains(r#"name="check_out""#),
        "{out}"
    );
    assert!(
        out.contains(
            r#"hx-get="/listings/villa-a/calendar?month=2027-02&amp;check_in=2027-02-02""#
        ),
        "{out}"
    );
    // January is the current month, so "previous" is disabled.
    assert!(
        out.contains(r#"aria-label="Previous month" disabled"#),
        "{out}"
    );
}

/// Keeps playwright/e2e/ui-js/fixtures/calendar.html (used to test the
/// calendar's ui.js behaviour) identical to what the component renders.
/// Regenerate with `UPDATE_FIXTURES=1 cargo test -p web_ui_common --test date_range_calendar`.
#[tokio::test]
async fn ui_js_fixture_matches_the_component() {
    let __cx = &Cx::default();
    let props = CalendarProps {
        listing_slug: "villa-a".into(),
        month: d(2027, 1, 1),
        months: 2,
        today: d(2027, 1, 5),
        ranges: vec![UnavailableRange {
            date_from: d(2027, 1, 10),
            date_to: d(2027, 1, 14),
        }],
        check_in: None,
        check_out: None,
    };
    let rendered = html(view! { date_range_calendar(props: props) }, __cx).await;
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../playwright/e2e/ui-js/fixtures/calendar.html"
    );
    if std::env::var("UPDATE_FIXTURES").is_ok() {
        assert!(std::fs::write(path, &rendered).is_ok(), "write {path}");
    }
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        committed, rendered,
        "fixture is stale; rerun with UPDATE_FIXTURES=1"
    );
}
