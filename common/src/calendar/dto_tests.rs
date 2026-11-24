//! Calendar DTO validation and redaction tests.

use super::dto::{
    BlockedDateError, CalendarFeedResponse, FeedNameError, NewBlockedDateRequest,
    NewCalendarFeedRequest, SyncOutcome, UpdateCalendarFeedRequest, clean_feed_name,
};
use super::types::FeedStatus;
use chrono::{NaiveDate, Utc};
use uuid::Uuid;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date")
}

fn today() -> NaiveDate {
    d("2026-12-01")
}

fn req(from: &str, to: &str, note: Option<&str>) -> NewBlockedDateRequest {
    NewBlockedDateRequest {
        date_from: d(from),
        date_to: d(to),
        note: note.map(str::to_string),
    }
}

#[test]
fn a_valid_block_is_accepted() {
    assert_eq!(
        req("2026-12-01", "2026-12-05", Some("owner stay")).validate_for(today()),
        Ok(())
    );
    assert_eq!(
        req("2027-01-01", "2028-01-01", None).validate_for(today()),
        Ok(())
    );
    assert_eq!(
        req("2028-11-25", "2028-12-01", None).validate_for(today()),
        Ok(())
    );
}

#[test]
fn invalid_blocks_are_refused() {
    let cases = [
        (
            req("2026-11-30", "2026-12-05", None),
            BlockedDateError::InPast,
        ),
        (
            req("2026-12-05", "2026-12-05", None),
            BlockedDateError::EmptyRange,
        ),
        (
            req("2026-12-05", "2026-12-04", None),
            BlockedDateError::EmptyRange,
        ),
        (
            req("2027-01-01", "2028-01-02", None),
            BlockedDateError::TooLong,
        ),
        (
            req("2028-11-25", "2028-12-02", None),
            BlockedDateError::TooFar,
        ),
        (
            req("2026-12-02", "2026-12-03", Some(&"n".repeat(501))),
            BlockedDateError::NoteTooLong,
        ),
    ];
    for (r, want) in cases {
        assert_eq!(r.validate_for(today()), Err(want), "{r:?}");
    }
    let ok = "é".repeat(500);
    assert_eq!(
        req("2026-12-02", "2026-12-03", Some(&ok)).validate_for(today()),
        Ok(())
    );
}

#[test]
fn feed_names_are_cleaned() {
    assert_eq!(clean_feed_name("  Airbnb\u{7}  "), Ok("Airbnb".to_string()));
    assert_eq!(clean_feed_name(" \n\t "), Err(FeedNameError::Empty));
    assert_eq!(
        clean_feed_name(&"a".repeat(61)),
        Err(FeedNameError::TooLong)
    );
    assert_eq!(
        clean_feed_name(&"é".repeat(60)).map(|n| n.chars().count()),
        Ok(60)
    );
}

#[test]
fn debug_output_never_shows_the_feed_url_path_or_query() {
    let url = "https://www.airbnb.com/calendar/ical/123.ics?s=topsecret";
    let feed = CalendarFeedResponse {
        id: Uuid::nil(),
        listing_id: Uuid::nil(),
        name: "Airbnb".into(),
        url: url.into(),
        is_active: true,
        last_status: FeedStatus::Ok,
        last_error: None,
        last_warning: None,
        last_attempt_at: None,
        last_success_at: None,
        event_count: 0,
        consecutive_failures: 0,
        created_at: Utc::now(),
    };
    let new = NewCalendarFeedRequest {
        name: "Airbnb".into(),
        url: url.into(),
    };
    let update = UpdateCalendarFeedRequest {
        url: Some(url.into()),
        ..Default::default()
    };
    for text in [
        format!("{feed:?}"),
        format!("{new:?}"),
        format!("{update:?}"),
    ] {
        assert!(text.contains("www.airbnb.com"), "{text}");
        assert!(
            !text.contains("topsecret") && !text.contains("ical"),
            "{text}"
        );
    }
}

#[test]
fn sync_outcomes_serialise_in_snake_case() {
    for (o, s) in [
        (SyncOutcome::Ok, "ok"),
        (SyncOutcome::Warning, "warning"),
        (SyncOutcome::Failed, "failed"),
        (SyncOutcome::Skipped, "skipped"),
        (SyncOutcome::Deferred, "deferred"),
    ] {
        assert_eq!(serde_json::to_value(o).expect("json"), s);
    }
}
