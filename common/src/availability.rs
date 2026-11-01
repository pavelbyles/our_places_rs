//! Booked date ranges for a listing, used to grey out nights in the
//! booking calendar.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A half-open range of booked nights: `date_from` is the first booked night
/// and `date_to` is the checkout day, which is free for a new check-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnavailableRange {
    /// First booked night.
    pub date_from: NaiveDate,
    /// Checkout day (not booked).
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
