//! The busy-only `.ics` export feed (spec §1.6.2): every event is "Reserved"
//! with raw dates, and no guest detail ever goes in.

use super::ics_parse::sha256_hex;
use chrono::{DateTime, NaiveDate, Utc};

/// Longest line, in octets, before folding (RFC 5545 §3.1).
const FOLD_OCTETS: usize = 75;
/// `PRODID` of the export feed.
pub const PRODID: &str = "-//Our Places//Calendar Sync//EN";

/// What an exported event stands for. Decides its `DESCRIPTION`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportSource {
    /// A confirmed stay or live hold here.
    Booking {
        /// The booking's reservation number.
        confirmation_code: String,
    },
    /// A manual block.
    Manual,
    /// A block imported from another channel's feed.
    Imported {
        /// The feed's display name.
        feed_name: String,
    },
}

/// One busy range in the export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportEvent {
    /// The event `UID`: ours (`…@ourplaces`) or an imported block's original.
    pub uid: String,
    /// First busy night.
    pub date_from: NaiveDate,
    /// The night after the last busy night.
    pub date_to: NaiveDate,
    /// What the range stands for.
    pub source: ExportSource,
}

impl ExportEvent {
    fn description(&self) -> String {
        match &self.source {
            ExportSource::Booking { confirmation_code } => {
                format!("Source: OurPlaces\nReservation #: {confirmation_code}")
            }
            ExportSource::Manual => "Source: OurPlaces".to_string(),
            ExportSource::Imported { feed_name } => {
                format!("Source: OurPlaces (via {})", strip_controls(feed_name))
            }
        }
    }
}

/// Renders the export calendar. Lines end in CRLF and are folded at 75 octets.
pub fn write_feed(calendar_name: &str, events: &[ExportEvent], stamp: DateTime<Utc>) -> String {
    let stamp = stamp.format("%Y%m%dT%H%M%SZ").to_string();
    let mut out = String::new();
    let mut line = |text: String| push_folded(&mut out, &text);
    line("BEGIN:VCALENDAR".into());
    line("VERSION:2.0".into());
    line(format!("PRODID:{PRODID}"));
    line("CALSCALE:GREGORIAN".into());
    line("METHOD:PUBLISH".into());
    line(format!(
        "X-WR-CALNAME:{}",
        escape_text(&strip_controls(calendar_name))
    ));
    for event in events {
        line("BEGIN:VEVENT".into());
        line(format!("UID:{}", escape_text(&event.uid)));
        line(format!("DTSTAMP:{stamp}"));
        line(format!(
            "DTSTART;VALUE=DATE:{}",
            event.date_from.format("%Y%m%d")
        ));
        line(format!(
            "DTEND;VALUE=DATE:{}",
            event.date_to.format("%Y%m%d")
        ));
        line("SUMMARY:Reserved".into());
        line(format!("DESCRIPTION:{}", escape_text(&event.description())));
        line("STATUS:CONFIRMED".into());
        line("TRANSP:OPAQUE".into());
        line("END:VEVENT".into());
    }
    line("END:VCALENDAR".into());
    out
}

/// A strong ETag for an export body, ignoring `DTSTAMP` lines so it only
/// changes when the events do.
pub fn etag(body: &str) -> String {
    let stable: Vec<&str> = body
        .split("\r\n")
        .filter(|line| !line.starts_with("DTSTAMP:"))
        .collect();
    let hash = sha256_hex(&[stable.join("\n").as_bytes()]);
    format!("\"{}\"", &hash[..32])
}

/// Removes control characters, newlines included.
fn strip_controls(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// RFC 5545 TEXT escaping. Other control characters are dropped.
fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Appends `text` folded so no line exceeds 75 octets, never splitting a
/// UTF-8 character. Continuation lines start with a space.
fn push_folded(out: &mut String, text: &str) {
    let mut used = 0;
    for c in text.chars() {
        let width = c.len_utf8();
        if used + width > FOLD_OCTETS {
            out.push_str("\r\n ");
            used = 1;
        }
        out.push(c);
        used += width;
    }
    out.push_str("\r\n");
}
