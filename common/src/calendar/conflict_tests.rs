//! Conflict classification and stale reason tests (spec §1.5).

use super::conflict::{StaleReason, stale_reason};
use super::types::ConflictKind;
use chrono::NaiveDate;

fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2027, 1, day).expect("date")
}

/// Stay is Jan 10 → Jan 15.
const STAY: (u32, u32) = (10, 15);

fn classify(from: u32, to: u32, gap: i32) -> Option<ConflictKind> {
    ConflictKind::classify((d(from), d(to)), (d(STAY.0), d(STAY.1)), gap)
}

#[test]
fn overlap_regardless_of_gap() {
    for gap in [0, 1, 3] {
        assert_eq!(
            classify(12, 13, gap),
            Some(ConflictKind::Overlap),
            "inside, gap {gap}"
        );
        assert_eq!(
            classify(8, 11, gap),
            Some(ConflictKind::Overlap),
            "start, gap {gap}"
        );
        assert_eq!(
            classify(14, 20, gap),
            Some(ConflictKind::Overlap),
            "end, gap {gap}"
        );
        assert_eq!(
            classify(1, 30, gap),
            Some(ConflictKind::Overlap),
            "around, gap {gap}"
        );
    }
}

#[test]
fn touching_ranges_are_turnover_only_with_a_gap() {
    // Block ends on check-in day / starts on check-out day.
    assert_eq!(classify(5, 10, 0), None);
    assert_eq!(classify(15, 18, 0), None);
    assert_eq!(classify(5, 10, 1), Some(ConflictKind::Turnover));
    assert_eq!(classify(15, 18, 1), Some(ConflictKind::Turnover));
    assert_eq!(classify(5, 10, 3), Some(ConflictKind::Turnover));
}

#[test]
fn the_gap_width_decides_turnover() {
    // Block ends 2 days before check-in.
    assert_eq!(classify(1, 8, 1), None);
    assert_eq!(classify(1, 8, 2), None);
    assert_eq!(classify(1, 8, 3), Some(ConflictKind::Turnover));
    // Block starts 2 days after check-out.
    assert_eq!(classify(17, 20, 1), None);
    assert_eq!(classify(17, 20, 3), Some(ConflictKind::Turnover));
}

#[test]
fn negative_gaps_count_as_zero() {
    assert_eq!(classify(5, 10, -4), None);
}

#[test]
fn stale_reasons_in_priority_order() {
    let stay = (d(10), d(15));
    assert_eq!(
        stale_reason(None, stay, false, 0),
        Some(StaleReason::EventRemoved)
    );
    assert_eq!(
        stale_reason(Some((d(12), d(13))), stay, false, 0),
        Some(StaleReason::BookingReleased)
    );
    assert_eq!(
        stale_reason(Some((d(20), d(22))), stay, true, 0),
        Some(StaleReason::NoLongerOverlapping)
    );
    assert_eq!(stale_reason(Some((d(12), d(13))), stay, true, 0), None);
    // A turnover-only conflict stays live while the gap is breached.
    assert_eq!(stale_reason(Some((d(15), d(17))), stay, true, 1), None);
    assert_eq!(
        stale_reason(Some((d(15), d(17))), stay, true, 0),
        Some(StaleReason::NoLongerOverlapping)
    );
}

#[test]
fn stale_codes_round_trip() {
    for r in [
        StaleReason::EventRemoved,
        StaleReason::BookingReleased,
        StaleReason::NoLongerOverlapping,
    ] {
        assert_eq!(StaleReason::from_code(r.code()), Some(r));
        assert_eq!(serde_json::to_value(r).expect("json"), r.code());
    }
}
