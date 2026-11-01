//! Tests for merging unavailable date ranges.

use chrono::NaiveDate;
use common::availability::{UnavailableRange, merge_ranges};

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2027, m, day).unwrap_or_default()
}

fn r(from: (u32, u32), to: (u32, u32)) -> UnavailableRange {
    UnavailableRange {
        date_from: d(from.0, from.1),
        date_to: d(to.0, to.1),
    }
}

#[test]
fn empty_input_gives_empty_output() {
    assert!(merge_ranges(Vec::new()).is_empty());
}

#[test]
fn disjoint_ranges_stay_separate_and_sorted() {
    let merged = merge_ranges(vec![r((3, 10), (3, 12)), r((1, 1), (1, 5))]);
    assert_eq!(merged, vec![r((1, 1), (1, 5)), r((3, 10), (3, 12))]);
}

#[test]
fn back_to_back_stays_merge() {
    // Checkout on the 5th and check-in on the 5th: nights 1-4 and 5-7 are contiguous.
    let merged = merge_ranges(vec![r((1, 1), (1, 5)), r((1, 5), (1, 8))]);
    assert_eq!(merged, vec![r((1, 1), (1, 8))]);
}

#[test]
fn overlapping_and_nested_ranges_merge() {
    let merged = merge_ranges(vec![
        r((2, 1), (2, 20)),
        r((2, 5), (2, 10)),
        r((2, 15), (2, 25)),
        r((4, 1), (4, 2)),
    ]);
    assert_eq!(merged, vec![r((2, 1), (2, 25)), r((4, 1), (4, 2))]);
}
