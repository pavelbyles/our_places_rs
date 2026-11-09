//! Export writer and ETag tests (spec §1.6.2–3).

use super::ics_parse::parse_feed;
use super::ics_write::{ExportEvent, ExportSource, etag, write_feed};
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date")
}

fn stamp(s: &str) -> DateTime<Utc> {
    s.parse().expect("timestamp")
}

fn events() -> Vec<ExportEvent> {
    vec![
        ExportEvent {
            uid: "booking-1@ourplaces".into(),
            date_from: d("2027-01-05"),
            date_to: d("2027-01-08"),
            source: ExportSource::Booking {
                confirmation_code: "ABC123".into(),
            },
        },
        ExportEvent {
            uid: "block-2@ourplaces".into(),
            date_from: d("2027-02-01"),
            date_to: d("2027-02-02"),
            source: ExportSource::Manual,
        },
        ExportEvent {
            uid: "abc@airbnb.com".into(),
            date_from: d("2027-03-01"),
            date_to: d("2027-03-04"),
            source: ExportSource::Imported {
                feed_name: "Airbnb\r\nX-EVIL:1".into(),
            },
        },
    ]
}

fn unfold(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in body.split("\r\n") {
        match (line.strip_prefix(' '), out.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => out.push(line.to_string()),
        }
    }
    out
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match (c, c == '\\') {
            (_, true) => match chars.next() {
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            },
            (c, false) => out.push(c),
        }
    }
    out
}

#[test]
fn headers_and_event_fields_are_present() {
    let body = write_feed("Villa Azul", &events(), stamp("2027-01-01T12:00:00Z"));
    let lines = unfold(&body);
    for want in [
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "PRODID:-//Our Places//Calendar Sync//EN",
        "CALSCALE:GREGORIAN",
        "METHOD:PUBLISH",
        "X-WR-CALNAME:Villa Azul",
        "UID:booking-1@ourplaces",
        "DTSTAMP:20270101T120000Z",
        "DTSTART;VALUE=DATE:20270105",
        "DTEND;VALUE=DATE:20270108",
        "SUMMARY:Reserved",
        "DESCRIPTION:Source: OurPlaces\\nReservation #: ABC123",
        "DESCRIPTION:Source: OurPlaces",
        "DESCRIPTION:Source: OurPlaces (via AirbnbX-EVIL:1)",
        "STATUS:CONFIRMED",
        "TRANSP:OPAQUE",
        "END:VCALENDAR",
    ] {
        assert!(lines.iter().any(|l| l == want), "missing {want}\n{body}");
    }
    assert_eq!(lines.iter().filter(|l| *l == "BEGIN:VEVENT").count(), 3);
    assert!(!lines.iter().any(|l| l.starts_with("X-EVIL")));
}

#[test]
fn every_line_is_crlf_and_at_most_75_octets() {
    let mut evs = events();
    evs.push(ExportEvent {
        uid: format!("{}é€😀", "x".repeat(200)),
        date_from: d("2027-04-01"),
        date_to: d("2027-04-02"),
        source: ExportSource::Imported {
            feed_name: "ñ".repeat(60),
        },
    });
    let name = "Villa ".to_string() + &"é".repeat(100);
    let body = write_feed(&name, &evs, stamp("2027-01-01T12:00:00Z"));
    assert!(body.ends_with("\r\n"));
    let without_last = &body[..body.len() - 2];
    for line in without_last.split("\r\n") {
        assert!(line.len() <= 75, "{} octets: {line}", line.len());
        assert!(!line.contains('\n') && !line.contains('\r'));
    }
    // Folding never splits a character: the body is still valid UTF-8 and
    // unfolds back to the original name.
    let lines = unfold(&body);
    assert!(lines.iter().any(|l| l == &format!("X-WR-CALNAME:{name}")));
}

#[test]
fn a_uid_with_specials_and_a_newline_round_trips() {
    let uid = "a,b;c\\d\ne".to_string();
    let body = write_feed(
        "V",
        &[ExportEvent {
            uid: uid.clone(),
            date_from: d("2027-01-05"),
            date_to: d("2027-01-06"),
            source: ExportSource::Manual,
        }],
        stamp("2027-01-01T00:00:00Z"),
    );
    let lines = unfold(&body);
    let written = lines
        .iter()
        .find_map(|l| l.strip_prefix("UID:"))
        .expect("uid line");
    assert_eq!(written, "a\\,b\\;c\\\\d\\ne");
    assert_eq!(unescape(written), uid);
}

#[test]
fn the_etag_ignores_the_stamp_and_follows_the_events() {
    let a = write_feed("V", &events(), stamp("2027-01-01T00:00:00Z"));
    let b = write_feed("V", &events(), stamp("2027-06-01T08:30:00Z"));
    assert_ne!(a, b);
    assert_eq!(etag(&a), etag(&b));
    let mut changed = events();
    changed[0].date_to = d("2027-01-09");
    let c = write_feed("V", &changed, stamp("2027-01-01T00:00:00Z"));
    assert_ne!(etag(&a), etag(&c));
    assert!(etag(&a).starts_with('"') && etag(&a).ends_with('"'));
}

#[test]
fn write_then_parse_round_trips_the_ranges() {
    let today = d("2026-12-01");
    let body = write_feed("V", &events(), stamp("2027-01-01T00:00:00Z"));
    let feed = parse_feed(body.as_bytes(), Uuid::from_u128(1), today).expect("parses");
    // Our own UIDs are skipped on import (echo protection); the imported one survives.
    let got: Vec<_> = feed
        .events
        .iter()
        .map(|e| (e.uid.as_str(), e.date_from, e.date_to))
        .collect();
    assert_eq!(
        got,
        vec![("abc@airbnb.com", d("2027-03-01"), d("2027-03-04"))]
    );

    let renamed: Vec<ExportEvent> = events()
        .into_iter()
        .enumerate()
        .map(|(i, e)| ExportEvent {
            uid: format!("other-{i}"),
            ..e
        })
        .collect();
    let body = write_feed("V", &renamed, stamp("2027-01-01T00:00:00Z"));
    let feed = parse_feed(body.as_bytes(), Uuid::from_u128(1), today).expect("parses");
    let mut got: Vec<_> = feed
        .events
        .iter()
        .map(|e| (e.date_from, e.date_to))
        .collect();
    got.sort();
    let mut want: Vec<_> = renamed.iter().map(|e| (e.date_from, e.date_to)).collect();
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn an_empty_export_is_still_a_valid_calendar() {
    let body = write_feed("V", &[], stamp("2027-01-01T00:00:00Z"));
    let feed = parse_feed(body.as_bytes(), Uuid::from_u128(1), d("2026-12-01")).expect("parses");
    assert!(feed.events.is_empty());
}
