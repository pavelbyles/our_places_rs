//! Stay dates: the unavailable ranges that grey out nights in the booking
//! calendar, and validating the dates and guests a stay is searched for.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A half-open range of nights a new stay can't use: booked nights, plus the
/// listing's gap on each side. `date_from` is the first such night and `date_to`
/// is the first day free for a new check-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnavailableRange {
    /// First unavailable night.
    pub date_from: NaiveDate,
    /// First day free for a new check-in.
    pub date_to: NaiveDate,
}

/// Unavailable nights for a listing within a requested window. Contains only
/// dates: no booking IDs, guest data or statuses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnavailableRangesResponse {
    /// Merged, sorted, non-overlapping ranges.
    pub ranges: Vec<UnavailableRange>,
}

/// Sorts ranges and merges any that overlap or touch, so the result is sorted
/// and non-overlapping. Runs in O(n log n).
pub fn merge_ranges(mut ranges: Vec<UnavailableRange>) -> Vec<UnavailableRange> {
    ranges.sort_by_key(|r| (r.date_from, r.date_to));
    ranges.into_iter().fold(Vec::new(), |mut merged, next| {
        match merged.last_mut() {
            Some(last) if next.date_from <= last.date_to => {
                last.date_to = last.date_to.max(next.date_to);
            }
            _ => merged.push(next),
        }
        merged
    })
}

/// Hours Jamaica is behind UTC. Jamaica has no daylight saving time.
const JAMAICA_UTC_OFFSET_HOURS: i64 = 5;

/// Longest search text accepted, in characters.
const MAX_SEARCH_TEXT_CHARS: usize = 200;

/// Today's date in Jamaica (UTC−5), the platform's calendar for stays.
pub fn jamaica_today(now: DateTime<Utc>) -> NaiveDate {
    (now - chrono::Duration::hours(JAMAICA_UTC_OFFSET_HOURS)).date_naive()
}

/// The instant a date starts in Jamaica (00:00 local, 05:00 UTC; Jamaica has
/// no daylight saving time).
pub fn jamaica_midnight_utc(date: NaiveDate) -> DateTime<Utc> {
    date.and_time(chrono::NaiveTime::MIN).and_utc()
        + chrono::Duration::hours(JAMAICA_UTC_OFFSET_HOURS)
}

/// Why stay dates, a guest count or search text were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StayInputError {
    /// A date couldn't be read as `YYYY-MM-DD`.
    #[error("Invalid date range: please enter valid dates.")]
    DateMalformed,
    /// Only one of the two dates was given.
    #[error("Invalid date range: choose both check-in and check-out.")]
    DateMissing,
    /// Check-out isn't after check-in.
    #[error("Invalid date range: check-out must be after check-in.")]
    CheckOutNotAfterCheckIn,
    /// Check-in is before today.
    #[error("Invalid date range: dates can't be in the past.")]
    InPast,
    /// The stay is longer than the limit.
    #[error("Invalid date range: stays can be at most {max_nights} nights.")]
    TooLong {
        /// The longest stay allowed.
        max_nights: i64,
    },
    /// The guest count isn't a whole number of at least 1.
    #[error("Guests must be at least 1.")]
    InvalidGuests,
    /// The search text is too long or has control characters.
    #[error("Please shorten your search and use only letters, numbers and punctuation.")]
    InvalidSearchText,
}

impl StayInputError {
    /// The machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DateMalformed
            | Self::DateMissing
            | Self::CheckOutNotAfterCheckIn
            | Self::InPast
            | Self::TooLong { .. } => "INVALID_STAY_DATES",
            Self::InvalidGuests => "INVALID_GUESTS",
            Self::InvalidSearchText => "INVALID_SEARCH_TEXT",
        }
    }

    /// The message a guest sees.
    pub fn message(&self) -> String {
        self.to_string()
    }
}

/// The message of a stay refusal in an API error's text, if it is one. The text
/// ends with the response body, such as `Failed (400): {"code":...,"message":...}`.
pub fn stay_refusal_message_in_error(text: &str) -> Option<String> {
    let body: serde_json::Value = serde_json::from_str(&text[text.find('{')?..]).ok()?;
    let code = body["code"].as_str()?;
    [
        "INVALID_STAY_DATES",
        "INVALID_GUESTS",
        "INVALID_SEARCH_TEXT",
        crate::pricing::MINIMUM_STAY_NOT_MET,
    ]
    .contains(&code)
    .then(|| body["message"].as_str().map(str::to_string))
    .flatten()
}

/// Checks a stay's dates. Search and stay pricing share this, so they can't
/// disagree on what counts as a valid stay.
pub fn validate_stay_dates(
    check_in: NaiveDate,
    check_out: NaiveDate,
    today: NaiveDate,
    max_nights: i64,
) -> Result<(), StayInputError> {
    if check_out <= check_in {
        return Err(StayInputError::CheckOutNotAfterCheckIn);
    }
    if check_in < today {
        return Err(StayInputError::InPast);
    }
    if (check_out - check_in).num_days() > max_nights {
        return Err(StayInputError::TooLong { max_nights });
    }
    Ok(())
}

/// Validated search input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaySearch {
    /// Check-in and check-out: both or neither, so a half-filled range can't exist.
    pub dates: Option<(NaiveDate, NaiveDate)>,
    /// Party size, at least 1.
    pub guests: Option<i32>,
    /// Trimmed search text.
    pub q: Option<String>,
}

/// A value that was given, ignoring surrounding space. An empty value counts as absent.
fn given(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

fn parse_date(value: Option<&str>) -> Result<Option<NaiveDate>, StayInputError> {
    given(value)
        .map(|v| {
            NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|_| StayInputError::DateMalformed)
        })
        .transpose()
}

fn parse_guests(value: Option<&str>) -> Result<Option<i32>, StayInputError> {
    given(value)
        .map(|v| {
            v.parse::<i32>()
                .ok()
                .filter(|guests| *guests >= 1)
                .ok_or(StayInputError::InvalidGuests)
        })
        .transpose()
}

fn parse_search_text(value: Option<&str>) -> Result<Option<String>, StayInputError> {
    given(value)
        .map(|v| {
            if v.chars().count() > MAX_SEARCH_TEXT_CHARS || v.chars().any(char::is_control) {
                Err(StayInputError::InvalidSearchText)
            } else {
                Ok(v.to_string())
            }
        })
        .transpose()
}

/// Reads raw search input, stopping at the first problem: dates, then guests,
/// then text. An empty value counts as absent.
pub fn parse_stay_search(
    check_in: Option<&str>,
    check_out: Option<&str>,
    guests: Option<&str>,
    q: Option<&str>,
    today: NaiveDate,
    max_nights: i64,
) -> Result<StaySearch, StayInputError> {
    let dates = match (parse_date(check_in)?, parse_date(check_out)?) {
        (None, None) => None,
        (Some(check_in), Some(check_out)) => {
            validate_stay_dates(check_in, check_out, today, max_nights)?;
            Some((check_in, check_out))
        }
        _ => return Err(StayInputError::DateMissing),
    };
    Ok(StaySearch {
        dates,
        guests: parse_guests(guests)?,
        q: parse_search_text(q)?,
    })
}

#[cfg(test)]
mod stay_input_tests {
    use super::*;
    use chrono::TimeZone;

    const MAX: i64 = 30;

    fn day(value: &str) -> NaiveDate {
        match NaiveDate::parse_from_str(value, "%Y-%m-%d") {
            Ok(date) => date,
            Err(e) => panic!("bad test date {value}: {e}"),
        }
    }

    fn today() -> NaiveDate {
        day("2030-06-15")
    }

    fn dates(check_in: &str, check_out: &str) -> Result<StaySearch, StayInputError> {
        parse_stay_search(Some(check_in), Some(check_out), None, None, today(), MAX)
    }

    fn guests(value: &str) -> Result<StaySearch, StayInputError> {
        parse_stay_search(None, None, Some(value), None, today(), MAX)
    }

    fn text(value: &str) -> Result<StaySearch, StayInputError> {
        parse_stay_search(None, None, None, Some(value), today(), MAX)
    }

    #[test]
    fn each_refusal_has_its_code_and_message() {
        for (error, code, message) in [
            (
                StayInputError::DateMalformed,
                "INVALID_STAY_DATES",
                "Invalid date range: please enter valid dates.",
            ),
            (
                StayInputError::DateMissing,
                "INVALID_STAY_DATES",
                "Invalid date range: choose both check-in and check-out.",
            ),
            (
                StayInputError::CheckOutNotAfterCheckIn,
                "INVALID_STAY_DATES",
                "Invalid date range: check-out must be after check-in.",
            ),
            (
                StayInputError::InPast,
                "INVALID_STAY_DATES",
                "Invalid date range: dates can't be in the past.",
            ),
            (
                StayInputError::TooLong { max_nights: 30 },
                "INVALID_STAY_DATES",
                "Invalid date range: stays can be at most 30 nights.",
            ),
            (
                StayInputError::InvalidGuests,
                "INVALID_GUESTS",
                "Guests must be at least 1.",
            ),
            (
                StayInputError::InvalidSearchText,
                "INVALID_SEARCH_TEXT",
                "Please shorten your search and use only letters, numbers and punctuation.",
            ),
        ] {
            assert_eq!(error.code(), code, "{error:?}");
            assert_eq!(error.message(), message, "{error:?}");
        }
    }

    #[test]
    fn checks_stop_at_the_first_problem_in_order() {
        // A malformed date comes before a missing one.
        assert_eq!(
            parse_stay_search(Some("soon"), None, None, None, today(), MAX),
            Err(StayInputError::DateMalformed)
        );
        // A missing date comes before guests and text.
        assert_eq!(
            parse_stay_search(
                Some("2030-07-01"),
                None,
                Some("0"),
                Some("\0"),
                today(),
                MAX
            ),
            Err(StayInputError::DateMissing)
        );
        // Order comes before the past: both are wrong here.
        assert_eq!(
            dates("2030-06-10", "2030-06-09"),
            Err(StayInputError::CheckOutNotAfterCheckIn)
        );
        // The past comes before length: both are wrong here.
        assert_eq!(
            dates("2030-06-10", "2031-06-10"),
            Err(StayInputError::InPast)
        );
        // Guests come before text.
        assert_eq!(
            parse_stay_search(None, None, Some("0"), Some("\0"), today(), MAX),
            Err(StayInputError::InvalidGuests)
        );
    }

    #[test]
    fn empty_values_count_as_absent() {
        assert_eq!(
            parse_stay_search(Some(""), Some("  "), Some(""), Some("   "), today(), MAX),
            Ok(StaySearch::default())
        );
        assert_eq!(
            parse_stay_search(None, None, None, None, today(), MAX),
            Ok(StaySearch::default())
        );
    }

    #[test]
    fn only_one_date_is_refused_either_way_round() {
        assert_eq!(
            parse_stay_search(None, Some("2030-07-01"), None, None, today(), MAX),
            Err(StayInputError::DateMissing)
        );
        assert_eq!(
            parse_stay_search(Some("2030-07-01"), Some(""), None, None, today(), MAX),
            Err(StayInputError::DateMissing)
        );
    }

    #[test]
    fn today_is_allowed_and_yesterday_is_not() {
        assert_eq!(
            dates("2030-06-15", "2030-06-16").map(|s| s.dates),
            Ok(Some((day("2030-06-15"), day("2030-06-16"))))
        );
        assert_eq!(
            dates("2030-06-14", "2030-06-16"),
            Err(StayInputError::InPast)
        );
    }

    #[test]
    fn check_out_must_be_after_check_in() {
        assert_eq!(
            dates("2030-07-01", "2030-07-01"),
            Err(StayInputError::CheckOutNotAfterCheckIn)
        );
    }

    #[test]
    fn the_longest_stay_is_allowed_and_one_more_night_is_not() {
        assert!(dates("2030-07-01", "2030-07-31").is_ok());
        assert_eq!(
            dates("2030-07-01", "2030-08-01"),
            Err(StayInputError::TooLong { max_nights: MAX })
        );
    }

    #[test]
    fn bad_dates_are_refused() {
        for bad in ["abc", "2030-02-30", "01/07/2030", "2030-7"] {
            assert_eq!(
                dates(bad, "2030-07-10"),
                Err(StayInputError::DateMalformed),
                "{bad}"
            );
        }
    }

    #[test]
    fn guests_must_be_a_whole_number_of_at_least_one() {
        assert_eq!(guests("1").map(|s| s.guests), Ok(Some(1)));
        for bad in ["0", "-1", "abc", "1.5", "2147483648"] {
            assert_eq!(guests(bad), Err(StayInputError::InvalidGuests), "{bad}");
        }
    }

    #[test]
    fn search_text_is_trimmed_and_limited() {
        assert_eq!(
            text("  Runaway Bay ").map(|s| s.q),
            Ok(Some("Runaway Bay".to_string()))
        );
        let longest = "é".repeat(200);
        assert_eq!(text(&longest).map(|s| s.q), Ok(Some(longest.clone())));
        assert_eq!(
            text(&format!("{longest}é")),
            Err(StayInputError::InvalidSearchText)
        );
        for bad in ["a\0b", "a\nb", "a\u{7f}b"] {
            assert_eq!(text(bad), Err(StayInputError::InvalidSearchText), "{bad:?}");
        }
    }

    #[test]
    fn a_stay_refusal_message_is_read_from_an_api_error() {
        let refused = r#"Failed (400 Bad Request): {"code":"INVALID_STAY_DATES","message":"Invalid date range: dates can't be in the past."}"#;
        assert_eq!(
            stay_refusal_message_in_error(refused).as_deref(),
            Some("Invalid date range: dates can't be in the past.")
        );
        let short = r#"Failed to quote stay (400 Bad Request): {"code":"MINIMUM_STAY_NOT_MET","message":"These dates need a stay of at least 5 nights.","required_nights":5}"#;
        assert_eq!(
            stay_refusal_message_in_error(short).as_deref(),
            Some("These dates need a stay of at least 5 nights.")
        );
        for other in [
            r#"Failed (409): {"code":"DATE_UNAVAILABLE","message":"gone"}"#,
            r#"Failed (400): {"error":"bad"}"#,
            "INVALID_STAY_DATES",
            "boom",
            "{ not json",
        ] {
            assert_eq!(stay_refusal_message_in_error(other), None, "{other}");
        }
    }

    #[test]
    fn jamaica_is_still_on_the_previous_date_early_in_the_utc_day() {
        let at = |hour| match Utc.with_ymd_and_hms(2030, 6, 15, hour, 0, 0).single() {
            Some(time) => time,
            None => panic!("bad test time"),
        };
        assert_eq!(jamaica_today(at(3)), day("2030-06-14"));
        assert_eq!(jamaica_today(at(4)), day("2030-06-14"));
        assert_eq!(jamaica_today(at(5)), day("2030-06-15"));
    }
}
