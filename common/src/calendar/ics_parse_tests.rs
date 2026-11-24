//! ICS feed parser tests (spec §1.3, §6).

use super::ics_parse::{
    ExternalEvent, FeedError, FeedWarning, MAX_FEED_BYTES, ParsedFeed, parse_feed,
};
use chrono::NaiveDate;
use std::collections::HashSet;
use uuid::Uuid;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date")
}

fn today() -> NaiveDate {
    d("2026-12-01")
}

fn feed_id() -> Uuid {
    Uuid::from_u128(7)
}

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/ical/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(path).expect("fixture")
}

fn parse(body: &str) -> Result<ParsedFeed, FeedError> {
    parse_feed(body.as_bytes(), feed_id(), today())
}

/// A calendar around the given VEVENT bodies, CRLF separated.
fn cal(events: &[&str]) -> String {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
    for e in events {
        out.push_str("BEGIN:VEVENT\r\n");
        out.push_str(e);
        if !e.ends_with("\r\n") {
            out.push_str("\r\n");
        }
        out.push_str("END:VEVENT\r\n");
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}

fn ranges(feed: &ParsedFeed) -> Vec<(String, NaiveDate, NaiveDate)> {
    feed.events
        .iter()
        .map(|e| (e.uid.clone(), e.date_from, e.date_to))
        .collect()
}

fn only(feed: &ParsedFeed) -> &ExternalEvent {
    assert_eq!(feed.events.len(), 1, "{feed:?}");
    &feed.events[0]
}

// ---- Fixtures --------------------------------------------------------------

#[test]
fn airbnb_fixture_gives_future_reservations() {
    let feed = parse_feed(&fixture("airbnb.ics"), feed_id(), today()).expect("parses");
    assert_eq!(
        ranges(&feed),
        vec![
            (
                "1418fb94e984-7d1a3e0b6c1e2b8d4d3b2f@airbnb.com".into(),
                d("2027-01-05"),
                d("2027-01-10")
            ),
            (
                "1418fb94e984-9e0c11aa22bb33cc44dd55@airbnb.com".into(),
                d("2027-02-20"),
                d("2027-03-01")
            ),
        ]
    );
    assert!(feed.warnings.is_empty());
}

#[test]
fn vrbo_fixture_reads_dates_and_utc_midnight_as_dates() {
    let feed = parse_feed(&fixture("vrbo.ics"), feed_id(), today()).expect("parses");
    assert_eq!(
        ranges(&feed),
        vec![
            (
                "6f2c1d1e-0b4f-4e43-9a0e-2b6a1a1c9d01@vrbo.com".into(),
                d("2027-01-15"),
                d("2027-01-20")
            ),
            (
                "6f2c1d1e-0b4f-4e43-9a0e-2b6a1a1c9d02@vrbo.com".into(),
                d("2027-02-01"),
                d("2027-02-04")
            ),
        ]
    );
}

#[test]
fn event_text_never_reaches_the_output() {
    for name in ["airbnb.ics", "vrbo.ics"] {
        let feed = parse_feed(&fixture(name), feed_id(), today()).expect("parses");
        let text = format!("{feed:?}");
        for secret in ["Jane", "Guest", "Phone", "1234", "HMABCDEF", "Reserved"] {
            assert!(!text.contains(secret), "{name}: {secret} leaked");
        }
    }
}

#[test]
fn recurring_events_use_the_first_occurrence_and_warn() {
    let feed = parse_feed(&fixture("google_rrule.ics"), feed_id(), today()).expect("parses");
    assert_eq!(
        ranges(&feed),
        vec![("weekly@google.com".into(), d("2027-01-05"), d("2027-01-07"))]
    );
    assert_eq!(feed.warnings, vec![FeedWarning::RecurrenceIgnored]);
}

#[test]
fn timed_events_cover_every_jamaica_night_they_touch() {
    let feed = parse_feed(&fixture("timed.ics"), feed_id(), today()).expect("parses");
    let mut got = ranges(&feed);
    got.sort_by_key(|r| r.1);
    assert_eq!(
        got,
        vec![
            // 20:00Z = 15:00 local; ends 16:00Z = 11:00 local on the 12th.
            ("utc@example.com".into(), d("2027-01-10"), d("2027-01-13")),
            ("tzid@example.com".into(), d("2027-01-20"), d("2027-01-23")),
            // Ends exactly at local midnight, so the 3rd isn't blocked.
            (
                "floating@example.com".into(),
                d("2027-02-01"),
                d("2027-02-03")
            ),
        ]
    );
}

#[test]
fn html_instead_of_ics_fails() {
    assert_eq!(
        parse_feed(&fixture("malformed.ics"), feed_id(), today()),
        Err(FeedError::NotCalendar)
    );
}

#[test]
fn a_truncated_body_fails() {
    assert_eq!(
        parse_feed(&fixture("truncated.ics"), feed_id(), today()),
        Err(FeedError::Truncated)
    );
}

// ---- Encoding --------------------------------------------------------------

#[test]
fn folded_lines_and_escapes_are_undone() {
    let feed = parse(&cal(&[
        "UID:a\\,b\\;c\\\\d\r\n  -e\r\nDTSTART;VALUE=DATE:20270105\r\nDTEND;VALUE=DATE:2027\r\n\t0107",
    ]))
    .expect("parses");
    let e = only(&feed);
    assert_eq!(e.uid, "a,b;c\\d -e");
    assert_eq!((e.date_from, e.date_to), (d("2027-01-05"), d("2027-01-07")));
}

#[test]
fn lf_only_lowercase_and_bom_are_accepted() {
    let body = "\u{feff}begin:vcalendar\nbegin:vevent\nuid:x@y\ndtstart;value=date:20270105\ndtend;value=date:20270106\nend:vevent\nend:vcalendar\n";
    let feed = parse(body).expect("parses");
    assert_eq!(only(&feed).uid, "x@y");
}

#[test]
fn invalid_utf8_fails() {
    let mut body = cal(&["UID:x\r\nDTSTART;VALUE=DATE:20270105"]).into_bytes();
    body.insert(20, 0xFF);
    assert_eq!(
        parse_feed(&body, feed_id(), today()),
        Err(FeedError::NotUtf8)
    );
}

#[test]
fn a_line_over_8_kb_fails() {
    let long = format!("DESCRIPTION:{}", "x".repeat(8 * 1024));
    let body = cal(&[&format!("UID:x\r\nDTSTART;VALUE=DATE:20270105\r\n{long}")]);
    assert_eq!(parse(&body), Err(FeedError::LineTooLong));
}

#[test]
fn a_body_over_2_mb_fails() {
    let body = vec![b'x'; MAX_FEED_BYTES + 1];
    assert_eq!(
        parse_feed(&body, feed_id(), today()),
        Err(FeedError::TooLarge)
    );
}

// ---- Dates -----------------------------------------------------------------

fn one(props: &str) -> (NaiveDate, NaiveDate) {
    let feed = parse(&cal(&[&format!("UID:x\r\n{props}")])).expect("parses");
    let e = only(&feed);
    (e.date_from, e.date_to)
}

#[test]
fn date_ranges() {
    assert_eq!(
        one("DTSTART;VALUE=DATE:20270105\r\nDTEND;VALUE=DATE:20270108"),
        (d("2027-01-05"), d("2027-01-08"))
    );
    // Bare 8-digit values are dates even without VALUE=DATE.
    assert_eq!(
        one("DTSTART:20270105\r\nDTEND:20270108"),
        (d("2027-01-05"), d("2027-01-08"))
    );
}

#[test]
fn inverted_empty_or_missing_end_gives_one_night() {
    let one_night = (d("2027-01-05"), d("2027-01-06"));
    assert_eq!(
        one("DTSTART;VALUE=DATE:20270105\r\nDTEND;VALUE=DATE:20270103"),
        one_night
    );
    assert_eq!(
        one("DTSTART;VALUE=DATE:20270105\r\nDTEND;VALUE=DATE:20270105"),
        one_night
    );
    assert_eq!(one("DTSTART;VALUE=DATE:20270105"), one_night);
    assert_eq!(
        one("DTSTART:20270105T150000Z\r\nDTEND:20270105T140000Z"),
        (d("2027-01-05"), d("2027-01-06"))
    );
}

#[test]
fn utc_midnight_is_a_date() {
    assert_eq!(
        one("DTSTART:20270105T000000Z\r\nDTEND:20270107T000000Z"),
        (d("2027-01-05"), d("2027-01-07"))
    );
}

#[test]
fn utc_times_move_to_jamaica_time() {
    // 03:00Z on the 5th is 22:00 on the 4th in Jamaica.
    assert_eq!(
        one("DTSTART:20270105T030000Z\r\nDTEND:20270107T050000Z"),
        (d("2027-01-04"), d("2027-01-07"))
    );
    // TZID=UTC behaves like Z.
    assert_eq!(
        one("DTSTART;TZID=UTC:20270105T030000\r\nDTEND;TZID=UTC:20270107T050000"),
        (d("2027-01-04"), d("2027-01-07"))
    );
}

#[test]
fn jamaica_unknown_tzid_and_floating_are_local() {
    for tz in ["TZID=America/Jamaica:", "TZID=\"Europe/Paris\":", ""] {
        let sep = if tz.is_empty() { ":" } else { ";" };
        let props = format!("DTSTART{sep}{tz}20270105T030000\r\nDTEND{sep}{tz}20270107T000000");
        assert_eq!(one(&props), (d("2027-01-05"), d("2027-01-07")), "{tz}");
    }
}

#[test]
fn durations() {
    assert_eq!(
        one("DTSTART;VALUE=DATE:20270105\r\nDURATION:P3D"),
        (d("2027-01-05"), d("2027-01-08"))
    );
    assert_eq!(
        one("DTSTART;VALUE=DATE:20270105\r\nDURATION:P1W"),
        (d("2027-01-05"), d("2027-01-12"))
    );
    // Anything else is rounded up to whole days.
    assert_eq!(
        one("DTSTART;VALUE=DATE:20270105\r\nDURATION:P1DT1H"),
        (d("2027-01-05"), d("2027-01-07"))
    );
    assert_eq!(
        one("DTSTART;VALUE=DATE:20270105\r\nDURATION:PT2H"),
        (d("2027-01-05"), d("2027-01-06"))
    );
    // A malformed duration is ignored: one night.
    assert_eq!(
        one("DTSTART;VALUE=DATE:20270105\r\nDURATION:-P3D"),
        (d("2027-01-05"), d("2027-01-06"))
    );
}

#[test]
fn a_missing_start_skips_the_event_and_warns() {
    let feed = parse(&cal(&[
        "UID:a\r\nDTEND;VALUE=DATE:20270108",
        "UID:b\r\nDTSTART;VALUE=DATE:garbage",
        "UID:c\r\nDTSTART;VALUE=DATE:20270105",
    ]))
    .expect("parses");
    assert_eq!(only(&feed).uid, "c");
    assert_eq!(feed.warnings, vec![FeedWarning::MissingStart]);
}

#[test]
fn dtstart_inside_valarm_or_vtimezone_is_ignored() {
    let body = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:X\r\nBEGIN:STANDARD\r\nDTSTART:20270101T000000\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:a\r\nBEGIN:VALARM\r\nDTSTART;VALUE=DATE:20270105\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let feed = parse(body).expect("parses");
    assert!(feed.events.is_empty());
    assert_eq!(feed.warnings, vec![FeedWarning::MissingStart]);
}

// ---- Event handling --------------------------------------------------------

#[test]
fn cancelled_events_are_skipped() {
    let feed = parse(&cal(&[
        "UID:a\r\nSTATUS:CANCELLED\r\nDTSTART;VALUE=DATE:20270105",
    ]))
    .expect("parses");
    assert!(feed.events.is_empty());
}

#[test]
fn rdate_and_exdate_warn_too() {
    for prop in ["RDATE:20270201", "EXDATE:20270112"] {
        let feed = parse(&cal(&[&format!(
            "UID:a\r\nDTSTART;VALUE=DATE:20270105\r\n{prop}"
        )]))
        .expect("parses");
        assert_eq!(feed.warnings, vec![FeedWarning::RecurrenceIgnored]);
    }
}

#[test]
fn our_own_uids_are_skipped() {
    let feed = parse(&cal(&[
        "UID:booking-123@ourplaces\r\nDTSTART;VALUE=DATE:20270105",
        "UID:BLOCK-9@OurPlaces\r\nDTSTART;VALUE=DATE:20270105",
        "UID:keep@ourplaces.example\r\nDTSTART;VALUE=DATE:20270105",
    ]))
    .expect("parses");
    assert_eq!(only(&feed).uid, "keep@ourplaces.example");
}

#[test]
fn the_window_drops_past_and_far_events_and_clips_only_the_end() {
    let feed = parse(&cal(&[
        // Ended before today.
        "UID:past\r\nDTSTART;VALUE=DATE:20261101\r\nDTEND;VALUE=DATE:20261201",
        // Ongoing: date_from stays in the past.
        "UID:ongoing\r\nDTSTART;VALUE=DATE:20261125\r\nDTEND;VALUE=DATE:20261205",
        // 2,000 nights: only date_to is clipped to today + 24 months.
        "UID:long\r\nDTSTART;VALUE=DATE:20261201\r\nDURATION:P2000D",
        // Starts after the horizon.
        "UID:far\r\nDTSTART;VALUE=DATE:20281202\r\nDTEND;VALUE=DATE:20281205",
        // Starts on the horizon: nothing left after clipping.
        "UID:edge\r\nDTSTART;VALUE=DATE:20281201\r\nDTEND;VALUE=DATE:20281205",
    ]))
    .expect("parses");
    assert_eq!(
        ranges(&feed),
        vec![
            ("long".into(), d("2026-12-01"), d("2028-12-01")),
            ("ongoing".into(), d("2026-11-25"), d("2026-12-05")),
        ]
    );
}

#[test]
fn feb_29_plus_24_months_is_feb_28() {
    let feed = parse_feed(
        cal(&["UID:a\r\nDTSTART;VALUE=DATE:20280220\r\nDTEND;VALUE=DATE:20280310"]).as_bytes(),
        feed_id(),
        d("2028-02-29"),
    )
    .expect("parses");
    assert_eq!(only(&feed).date_to, d("2028-03-10"));
    let feed = parse_feed(
        cal(&["UID:a\r\nDTSTART;VALUE=DATE:20300101\r\nDTEND;VALUE=DATE:20300310"]).as_bytes(),
        feed_id(),
        d("2028-02-29"),
    )
    .expect("parses");
    assert_eq!(only(&feed).date_to, d("2030-02-28"));
}

// ---- UIDs ------------------------------------------------------------------

#[test]
fn uids_fixture_numbers_duplicates_and_moves_past_real_suffixes() {
    let feed = parse_feed(&fixture("uids.ics"), feed_id(), today()).expect("parses");
    let mut got = ranges(&feed);
    got.sort_by_key(|r| r.1);
    assert_eq!(got.len(), 4, "{got:?}");
    assert_eq!(
        got[0],
        ("dup@example.com".into(), d("2027-02-01"), d("2027-02-03"))
    );
    assert_eq!(
        got[1],
        ("dup@example.com#3".into(), d("2027-03-01"), d("2027-03-03"))
    );
    assert_eq!(
        got[2],
        ("dup@example.com#2".into(), d("2027-04-01"), d("2027-04-03"))
    );
    assert!(got[3].0.starts_with("op-derived-"), "{:?}", got[3]);
}

#[test]
fn a_missing_uid_is_derived_from_feed_and_dates() {
    let body = cal(&["DTSTART;VALUE=DATE:20270105\r\nDTEND;VALUE=DATE:20270107"]);
    let a = parse(&body).expect("parses");
    let b = parse(&body).expect("parses");
    assert_eq!(a, b);
    let other = parse_feed(body.as_bytes(), Uuid::from_u128(8), today()).expect("parses");
    assert_ne!(only(&a).uid, only(&other).uid);
    assert_eq!(only(&a).uid.len(), "op-derived-".len() + 64);
}

#[test]
fn duplicates_are_numbered_in_date_order_whatever_the_feed_order() {
    let e1 = "UID:same\r\nDTSTART;VALUE=DATE:20270301\r\nDTEND;VALUE=DATE:20270302";
    let e2 = "UID:same\r\nDTSTART;VALUE=DATE:20270101\r\nDTEND;VALUE=DATE:20270102";
    let e3 = "UID:same\r\nDTSTART;VALUE=DATE:20270201\r\nDTEND;VALUE=DATE:20270202";
    let a = parse(&cal(&[e1, e2, e3])).expect("parses");
    let b = parse(&cal(&[e3, e1, e2])).expect("parses");
    assert_eq!(a, b);
    let mut got = ranges(&a);
    got.sort_by_key(|r| r.1);
    let names: Vec<_> = got.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(names, vec!["same", "same#2", "same#3"]);
}

#[test]
fn long_empty_and_control_character_uids_are_normalised() {
    let long = "u".repeat(501);
    let feed = parse(&cal(&[
        &format!("UID:{long}\r\nDTSTART;VALUE=DATE:20270105"),
        "UID:\u{7}\u{1b}\r\nDTSTART;VALUE=DATE:20270106",
        "UID:ab\u{1}c\r\nDTSTART;VALUE=DATE:20270107",
        "UID:line\\nbreak\r\nDTSTART;VALUE=DATE:20270108",
    ]))
    .expect("parses");
    let mut got = ranges(&feed);
    got.sort_by_key(|r| r.1);
    assert!(
        got[0].0.starts_with("op-h-") && got[0].0.len() == 69,
        "{}",
        got[0].0
    );
    assert!(got[1].0.starts_with("op-h-"), "{}", got[1].0);
    assert_eq!(got[2].0, "abc");
    assert_eq!(got[3].0, "linebreak");
}

#[test]
fn exactly_500_bytes_is_kept() {
    let uid = "u".repeat(500);
    let feed = parse(&cal(&[&format!(
        "UID:{uid}\r\nDTSTART;VALUE=DATE:20270105"
    )]))
    .expect("parses");
    assert_eq!(only(&feed).uid, uid);
}

/// Every combination of these UID shapes, repeated, gives unique UIDs of at
/// most 512 bytes.
#[test]
fn output_uids_are_always_unique_and_bounded() {
    let max = "m".repeat(500);
    let shapes: Vec<String> = vec![
        "a".into(),
        "a#2".into(),
        "a#3".into(),
        max.clone(),
        format!("{max}#2"),
        "x".repeat(600),
        "\u{1}".into(),
        "".into(),
    ];
    for repeat in 1..=4 {
        let mut events = Vec::new();
        for (i, shape) in shapes.iter().enumerate() {
            for r in 0..repeat {
                let day = 1 + (i * 4 + r) % 28;
                let uid_line = if shape.is_empty() {
                    String::new()
                } else {
                    format!("UID:{shape}\r\n")
                };
                events.push(format!("{uid_line}DTSTART;VALUE=DATE:202702{day:02}"));
            }
        }
        let refs: Vec<&str> = events.iter().map(String::as_str).collect();
        let feed = parse(&cal(&refs)).expect("parses");
        assert_eq!(feed.events.len(), events.len());
        let unique: HashSet<_> = feed.events.iter().map(|e| e.uid.as_str()).collect();
        assert_eq!(unique.len(), feed.events.len(), "repeat {repeat}");
        assert!(feed.events.iter().all(|e| e.uid.len() <= 512));
    }
}

// ---- Failures --------------------------------------------------------------

#[test]
fn more_than_5000_events_fails() {
    let event = "UID:x\r\nDTSTART;VALUE=DATE:20270105";
    let ok: Vec<&str> = vec![event; 5000];
    assert_eq!(parse(&cal(&ok)).expect("5000 is fine").events.len(), 5000);
    let too_many: Vec<&str> = vec![event; 5001];
    assert_eq!(parse(&cal(&too_many)), Err(FeedError::TooManyEvents));
}

#[test]
fn structural_failures() {
    assert_eq!(
        parse("BEGIN:VEVENT\r\nEND:VEVENT\r\n"),
        Err(FeedError::NotCalendar)
    );
    assert_eq!(parse(""), Err(FeedError::NotCalendar));
    assert_eq!(
        parse("BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nEND:VCALENDAR\r\n"),
        Err(FeedError::Truncated)
    );
    assert_eq!(
        parse("BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nEND:VEVENT\r\n"),
        Err(FeedError::Truncated)
    );
    assert_eq!(
        parse("BEGIN:VCALENDAR\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"),
        Err(FeedError::Truncated)
    );
}

#[test]
fn an_empty_calendar_is_a_success_with_no_events() {
    let feed = parse("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n").expect("parses");
    assert_eq!(feed, ParsedFeed::default());
}

#[test]
fn error_codes_and_messages_are_stable() {
    for e in [
        FeedError::TooLarge,
        FeedError::NotUtf8,
        FeedError::LineTooLong,
        FeedError::NotCalendar,
        FeedError::Truncated,
        FeedError::TooManyEvents,
    ] {
        assert!(e.code().starts_with("feed_"));
        assert!(!e.to_string().contains("http"));
    }
}
