//! When an imported block collides with a stay, and when that collision has
//! gone stale (spec §1.5). The database computes the same rules in SQL; these
//! are the reference the tests pin down.

use super::types::ConflictKind;
use chrono::{NaiveDate, TimeDelta};
use serde::{Deserialize, Serialize};

/// Largest turnover gap honoured, in days.
pub const MAX_GAP_DAYS: i32 = 365;

/// Nights `[from, to)`.
pub type NightRange = (NaiveDate, NaiveDate);

/// The turnover gap as availability uses it: clamped to `0..=365`.
pub fn clamp_gap(gap: i32) -> i64 {
    i64::from(gap.clamp(0, MAX_GAP_DAYS))
}

impl ConflictKind {
    /// How an imported block meets a stay, given the listing's turnover gap.
    /// `None` when they're far enough apart.
    pub fn classify(block: NightRange, stay: NightRange, gap: i32) -> Option<ConflictKind> {
        let g = TimeDelta::days(clamp_gap(gap));
        let (from, to) = block;
        let (check_in, check_out) = stay;
        if from < check_out && to > check_in {
            return Some(ConflictKind::Overlap);
        }
        let widened_from = from.checked_sub_signed(g).unwrap_or(NaiveDate::MIN);
        let widened_to = to.checked_add_signed(g).unwrap_or(NaiveDate::MAX);
        (widened_from < check_out && widened_to > check_in).then_some(ConflictKind::Turnover)
    }
}

/// Why an open conflict no longer needs attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StaleReason {
    /// The event left its feed, or the feed was removed.
    EventRemoved,
    /// The booking is no longer confirmed or a live hold.
    BookingReleased,
    /// The block's current dates no longer reach the stay.
    NoLongerOverlapping,
}

impl StaleReason {
    /// Stable code, as stored and sent.
    pub fn code(&self) -> &'static str {
        match self {
            StaleReason::EventRemoved => "event_removed",
            StaleReason::BookingReleased => "booking_released",
            StaleReason::NoLongerOverlapping => "no_longer_overlapping",
        }
    }

    /// Parses [`StaleReason::code`].
    pub fn from_code(code: &str) -> Option<StaleReason> {
        match code {
            "event_removed" => Some(StaleReason::EventRemoved),
            "booking_released" => Some(StaleReason::BookingReleased),
            "no_longer_overlapping" => Some(StaleReason::NoLongerOverlapping),
            _ => None,
        }
    }

    /// Short text for the host.
    pub fn label(&self) -> &'static str {
        match self {
            StaleReason::EventRemoved => "calendar event removed",
            StaleReason::BookingReleased => "booking no longer active",
            StaleReason::NoLongerOverlapping => "dates no longer overlap",
        }
    }
}

/// Why a conflict is stale, if it is. `block` is the block's current range,
/// `None` once the block is gone; `booking_live` is whether the stay is
/// confirmed or a live hold.
pub fn stale_reason(
    block: Option<NightRange>,
    stay: NightRange,
    booking_live: bool,
    gap: i32,
) -> Option<StaleReason> {
    let Some(block) = block else {
        return Some(StaleReason::EventRemoved);
    };
    if !booking_live {
        return Some(StaleReason::BookingReleased);
    }
    ConflictKind::classify(block, stay, gap)
        .is_none()
        .then_some(StaleReason::NoLongerOverlapping)
}
