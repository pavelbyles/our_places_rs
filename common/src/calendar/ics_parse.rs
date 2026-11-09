//! A small, hand-written subset of RFC 5545 for imported availability feeds.
//!
//! Only the top-level properties of each `VEVENT` are read. Event text
//! (`SUMMARY`, `DESCRIPTION`, …) may hold guest names, so it's never kept:
//! the output holds UIDs and night ranges only. See spec §1.3 for the rules.

use crate::reference::LISTING_UTC_OFFSET_SECS;
use chrono::{Months, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use thiserror::Error;
use uuid::Uuid;

/// Largest feed body accepted, in bytes.
pub const MAX_FEED_BYTES: usize = 2 * 1024 * 1024;
/// Most `VEVENT`s accepted in one feed.
pub const MAX_FEED_EVENTS: usize = 5_000;
/// Longest physical line accepted, in bytes.
pub const MAX_LINE_BYTES: usize = 8 * 1024;
/// How far ahead imported events are kept.
pub const FEED_WINDOW_MONTHS: u32 = 24;
/// Longest UID kept as is; longer ones are replaced by a hash.
const MAX_RAW_UID_BYTES: usize = 500;
/// Suffix of UIDs we export, skipped on import so our own events can't echo back.
pub const OUR_UID_SUFFIX: &str = "@ourplaces";

/// Why a feed body was refused as a whole. Its existing blocks are kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FeedError {
    /// The body is larger than [`MAX_FEED_BYTES`].
    #[error("feed is larger than 2 MB")]
    TooLarge,
    /// The body isn't UTF-8.
    #[error("feed is not UTF-8 text")]
    NotUtf8,
    /// A line is longer than [`MAX_LINE_BYTES`].
    #[error("feed has a line longer than 8 KB")]
    LineTooLong,
    /// There's no `BEGIN:VCALENDAR`, so the body isn't a calendar.
    #[error("response is not a calendar")]
    NotCalendar,
    /// Components aren't balanced, so the body was cut off or is malformed.
    #[error("calendar is incomplete or malformed")]
    Truncated,
    /// The feed has more than [`MAX_FEED_EVENTS`] events.
    #[error("feed has more than 5000 events")]
    TooManyEvents,
}

impl FeedError {
    /// Stable code, stored as the feed's `last_error`.
    pub fn code(&self) -> &'static str {
        match self {
            FeedError::TooLarge => "feed_too_large",
            FeedError::NotUtf8 => "feed_not_utf8",
            FeedError::LineTooLong => "feed_line_too_long",
            FeedError::NotCalendar => "feed_not_calendar",
            FeedError::Truncated => "feed_truncated",
            FeedError::TooManyEvents => "feed_too_many_events",
        }
    }
}

/// Something in a feed was skipped or simplified, but the feed still synced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FeedWarning {
    /// An event had no usable `DTSTART` and was skipped.
    MissingStart,
    /// An event repeats; only its first occurrence was used.
    RecurrenceIgnored,
}

impl FeedWarning {
    /// Stable code.
    pub fn code(&self) -> &'static str {
        match self {
            FeedWarning::MissingStart => "event_missing_start",
            FeedWarning::RecurrenceIgnored => "recurrence_ignored",
        }
    }

    /// One-line text for the host.
    pub fn message(&self) -> &'static str {
        match self {
            FeedWarning::MissingStart => "some events have no start date and were skipped",
            FeedWarning::RecurrenceIgnored => {
                "some events repeat; only their first occurrence blocks dates"
            }
        }
    }
}

/// One imported event, as nights `[date_from, date_to)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExternalEvent {
    /// Normalised UID: unique within the feed and at most 512 bytes.
    pub uid: String,
    /// First blocked night.
    pub date_from: NaiveDate,
    /// The night after the last blocked night.
    pub date_to: NaiveDate,
}

/// A feed's events and warnings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedFeed {
    /// Events within the window, with unique UIDs, sorted by UID.
    pub events: Vec<ExternalEvent>,
    /// Distinct warnings, sorted.
    pub warnings: Vec<FeedWarning>,
}

/// Parses a feed body fetched for `feed_id`, keeping events that touch
/// `[today, today + 24 months)`.
pub fn parse_feed(body: &[u8], feed_id: Uuid, today: NaiveDate) -> Result<ParsedFeed, FeedError> {
    if body.len() > MAX_FEED_BYTES {
        return Err(FeedError::TooLarge);
    }
    let body = body.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(body);
    let text = std::str::from_utf8(body).map_err(|_| FeedError::NotUtf8)?;
    let lines = unfold(text)?;
    let raw = read_events(&lines)?;

    let mut warnings = HashSet::new();
    let candidates: Vec<(Option<String>, NaiveDate, NaiveDate)> = raw
        .into_iter()
        .filter_map(|event| event.into_range(&mut warnings))
        .collect();
    let named = normalise_uids(candidates, feed_id);

    let horizon = today
        .checked_add_months(Months::new(FEED_WINDOW_MONTHS))
        .unwrap_or(NaiveDate::MAX);
    let mut events: Vec<ExternalEvent> = named
        .into_iter()
        .filter(|e| e.date_to > today && e.date_from < horizon)
        .map(|e| ExternalEvent {
            date_to: e.date_to.min(horizon),
            ..e
        })
        .collect();
    events.sort();
    let mut warnings: Vec<FeedWarning> = warnings.into_iter().collect();
    warnings.sort();
    Ok(ParsedFeed { events, warnings })
}

/// Splits into logical lines: CRLF or LF endings, and continuation lines
/// (starting with a space or tab) joined onto the previous one.
fn unfold(text: &str) -> Result<Vec<String>, FeedError> {
    let mut lines: Vec<String> = Vec::new();
    for physical in text.split('\n') {
        let physical = physical.strip_suffix('\r').unwrap_or(physical);
        if physical.len() > MAX_LINE_BYTES {
            return Err(FeedError::LineTooLong);
        }
        match (physical.strip_prefix([' ', '\t']), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ if physical.trim().is_empty() => {}
            _ => lines.push(physical.to_string()),
        }
    }
    Ok(lines)
}

/// A content line: upper-cased name, upper-cased parameter names with their
/// values, and the raw value.
struct ContentLine<'a> {
    name: String,
    params: Vec<(String, &'a str)>,
    value: &'a str,
}

impl ContentLine<'_> {
    fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.trim_matches('"'))
    }
}

/// Splits `NAME;P=V;P2="a:b":value`, honouring quoted parameter values.
fn split_line(line: &str) -> Option<ContentLine<'_>> {
    let mut in_quotes = false;
    let mut colon = None;
    let mut semis = Vec::new();
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ';' if !in_quotes => semis.push(i),
            ':' if !in_quotes => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let head = &line[..colon];
    let name_end = semis.first().copied().unwrap_or(colon);
    let mut params = Vec::new();
    let bounds: Vec<usize> = semis
        .iter()
        .copied()
        .chain(std::iter::once(colon))
        .collect();
    for pair in bounds.windows(2) {
        let part = &line[pair[0] + 1..pair[1]];
        if let Some((n, v)) = part.split_once('=') {
            params.push((n.trim().to_ascii_uppercase(), v));
        }
    }
    Some(ContentLine {
        name: head[..name_end].trim().to_ascii_uppercase(),
        params,
        value: &line[colon + 1..],
    })
}

/// Undoes RFC 5545 text escaping.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// A date property value as written.
#[derive(Debug, Clone, Copy)]
enum DateValue {
    /// A whole day.
    Date(NaiveDate),
    /// A local (Jamaica) date and time.
    Local(NaiveDateTime),
}

/// The top-level properties of one `VEVENT` that matter here.
#[derive(Default)]
struct RawEvent {
    uid: Option<String>,
    start: Option<Option<DateValue>>,
    end: Option<DateValue>,
    duration: Option<TimeDelta>,
    cancelled: bool,
    recurs: bool,
}

/// Walks the components, collecting each `VEVENT`'s top-level properties.
fn read_events(lines: &[String]) -> Result<Vec<RawEvent>, FeedError> {
    let mut stack: Vec<String> = Vec::new();
    let mut seen_calendar = false;
    let mut events = Vec::new();
    let mut current: Option<RawEvent> = None;

    for line in lines {
        let Some(cl) = split_line(line) else {
            continue;
        };
        match cl.name.as_str() {
            "BEGIN" => {
                let component = cl.value.trim().to_ascii_uppercase();
                match (component.as_str(), stack.last().map(String::as_str)) {
                    ("VCALENDAR", None) => seen_calendar = true,
                    ("VCALENDAR", Some(_)) => return Err(FeedError::Truncated),
                    (_, None) => return Err(FeedError::NotCalendar),
                    ("VEVENT", Some("VCALENDAR")) => {
                        if events.len() >= MAX_FEED_EVENTS {
                            return Err(FeedError::TooManyEvents);
                        }
                        current = Some(RawEvent::default());
                    }
                    _ => {}
                }
                stack.push(component);
            }
            "END" => {
                let component = cl.value.trim().to_ascii_uppercase();
                if stack.pop().as_deref() != Some(component.as_str()) {
                    return Err(FeedError::Truncated);
                }
                if component == "VEVENT" && stack.last().map(String::as_str) == Some("VCALENDAR") {
                    events.extend(current.take());
                }
            }
            _ => {
                if let (Some(event), Some("VEVENT")) =
                    (current.as_mut(), stack.last().map(String::as_str))
                {
                    event.read(&cl);
                }
            }
        }
    }
    match (seen_calendar, stack.is_empty()) {
        (false, _) => Err(FeedError::NotCalendar),
        (true, false) => Err(FeedError::Truncated),
        (true, true) => Ok(events),
    }
}

impl RawEvent {
    fn read(&mut self, cl: &ContentLine<'_>) {
        match cl.name.as_str() {
            "UID" => self.uid = Some(unescape(cl.value)),
            "DTSTART" => self.start = Some(parse_date_value(cl)),
            "DTEND" => self.end = parse_date_value(cl),
            "DURATION" => self.duration = parse_duration(cl.value.trim()),
            "STATUS" => self.cancelled = cl.value.trim().eq_ignore_ascii_case("CANCELLED"),
            "RRULE" | "RDATE" | "EXDATE" => self.recurs = true,
            _ => {}
        }
    }

    /// The event's nights, or `None` when it's skipped.
    fn into_range(
        self,
        warnings: &mut HashSet<FeedWarning>,
    ) -> Option<(Option<String>, NaiveDate, NaiveDate)> {
        if self.cancelled {
            return None;
        }
        if self
            .uid
            .as_deref()
            .is_some_and(|uid| uid.trim().to_ascii_lowercase().ends_with(OUR_UID_SUFFIX))
        {
            return None;
        }
        let Some(Some(start)) = self.start else {
            warnings.insert(FeedWarning::MissingStart);
            return None;
        };
        if self.recurs {
            warnings.insert(FeedWarning::RecurrenceIgnored);
        }
        let start_local = match start {
            DateValue::Date(d) => d.and_time(NaiveTime::MIN),
            DateValue::Local(dt) => dt,
        };
        let date_from = start_local.date();
        let end_local = match (self.end, self.duration) {
            (Some(DateValue::Date(d)), _) => Some(d.and_time(NaiveTime::MIN)),
            (Some(DateValue::Local(dt)), _) => Some(dt),
            (None, Some(duration)) => start_local.checked_add_signed(duration),
            (None, None) => None,
        };
        let date_to = end_local
            .and_then(|end| match end.time() == NaiveTime::MIN {
                true => Some(end.date()),
                false => end.date().succ_opt(),
            })
            .filter(|to| *to > date_from)
            .or_else(|| date_from.succ_opt())?;
        Some((self.uid, date_from, date_to))
    }
}

/// Parses `DTSTART`/`DTEND`. UTC times are moved to Jamaica time, except
/// exactly midnight UTC, which channels use for all-day events.
fn parse_date_value(cl: &ContentLine<'_>) -> Option<DateValue> {
    let value = cl.value.trim();
    let is_date = cl
        .param("VALUE")
        .is_some_and(|v| v.eq_ignore_ascii_case("DATE"))
        || (value.len() == 8 && value.bytes().all(|b| b.is_ascii_digit()));
    if is_date {
        return NaiveDate::parse_from_str(value.get(..8)?, "%Y%m%d")
            .ok()
            .map(DateValue::Date);
    }
    let (stamp, zulu) = match value.strip_suffix(['Z', 'z']) {
        Some(stamp) => (stamp, true),
        None => (value, false),
    };
    let dt = NaiveDateTime::parse_from_str(stamp, "%Y%m%dT%H%M%S").ok()?;
    let utc = zulu || cl.param("TZID").is_some_and(is_utc_tzid);
    match (utc, dt.time() == NaiveTime::MIN) {
        (true, true) => Some(DateValue::Date(dt.date())),
        (true, false) => dt
            .checked_add_signed(TimeDelta::seconds(i64::from(LISTING_UTC_OFFSET_SECS)))
            .map(DateValue::Local),
        // Jamaica, other zones and floating times are all read as local.
        (false, _) => Some(DateValue::Local(dt)),
    }
}

fn is_utc_tzid(tzid: &str) -> bool {
    let tzid = tzid.trim().trim_start_matches('/');
    [
        "UTC",
        "Etc/UTC",
        "GMT",
        "Etc/GMT",
        "Z",
        "Zulu",
        "Universal",
        "Etc/Universal",
    ]
    .iter()
    .any(|alias| alias.eq_ignore_ascii_case(tzid))
}

/// Parses `[+]P[nW][nD][T[nH][nM][nS]]`. Negative durations are refused.
fn parse_duration(value: &str) -> Option<TimeDelta> {
    let rest = value.strip_prefix('+').unwrap_or(value);
    let rest = rest.strip_prefix(['P', 'p'])?;
    let mut seconds: i64 = 0;
    let mut number = String::new();
    let mut in_time = false;
    for c in rest.chars() {
        match c.to_ascii_uppercase() {
            d if d.is_ascii_digit() => number.push(d),
            'T' => in_time = true,
            unit => {
                let n: i64 = number.parse().ok()?;
                number.clear();
                let per = match (unit, in_time) {
                    ('W', false) => 7 * 86_400,
                    ('D', false) => 86_400,
                    ('H', true) => 3_600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
                seconds = seconds.checked_add(n.checked_mul(per)?)?;
            }
        }
    }
    match number.is_empty() {
        true => TimeDelta::try_seconds(seconds),
        false => None,
    }
}

fn sha256_hex(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Gives every event a unique UID of at most 512 bytes (spec §1.3).
fn normalise_uids(
    events: Vec<(Option<String>, NaiveDate, NaiveDate)>,
    feed_id: Uuid,
) -> Vec<ExternalEvent> {
    let mut groups: BTreeMap<String, Vec<(NaiveDate, NaiveDate)>> = BTreeMap::new();
    for (uid, from, to) in events {
        let base = match uid {
            None => format!(
                "op-derived-{}",
                sha256_hex(&[
                    feed_id.as_bytes(),
                    from.to_string().as_bytes(),
                    to.to_string().as_bytes(),
                ])
            ),
            Some(raw) => {
                let cleaned: String = raw.trim().chars().filter(|c| !c.is_control()).collect();
                match cleaned.is_empty() || raw.len() > MAX_RAW_UID_BYTES {
                    true => format!("op-h-{}", sha256_hex(&[raw.as_bytes()])),
                    false => cleaned,
                }
            }
        };
        groups.entry(base).or_default().push((from, to));
    }

    let mut used: HashSet<String> = groups.keys().cloned().collect();
    let mut out = Vec::new();
    for (base, mut ranges) in groups {
        ranges.sort();
        let mut next = 2u32;
        for (i, (date_from, date_to)) in ranges.into_iter().enumerate() {
            let uid = match i {
                0 => base.clone(),
                _ => loop {
                    let candidate = format!("{base}#{next}");
                    next += 1;
                    if used.insert(candidate.clone()) {
                        break candidate;
                    }
                },
            };
            out.push(ExternalEvent {
                uid,
                date_from,
                date_to,
            });
        }
    }
    out
}
