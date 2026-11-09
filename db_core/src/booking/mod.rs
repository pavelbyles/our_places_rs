//! Booking domain entities and operations: pending holds, lookups, status
//! transitions, availability and the immutable booking history.

mod availability;
mod history;
mod hold;
mod queries;
mod update;

pub use availability::{check_availability, check_availability_excluding, get_unavailable_ranges};
pub use history::get_booking_history;
pub(crate) use history::record_booking_history;
pub(crate) use hold::{cancel_guest_holds, insert_hold};
pub use hold::{
    create_booking, create_booking_superseding, expire_stale_holds, transfer_booking_guest,
};
pub use queries::{
    get_booking_by_id, get_bookings, get_bookings_by_host_id, get_bookings_by_listing_id,
    get_bookings_by_user_id, get_confirmed_bookings_past_checkout,
};
pub use update::{delete_booking, update_booking};

#[cfg(test)]
mod availability_parity_tests;
#[cfg(test)]
mod gap_calendar_tests;
#[cfg(test)]
mod gap_tests;
#[cfg(test)]
mod hold_expiry_tests;
#[cfg(test)]
mod hold_gap_tests;
#[cfg(test)]
mod host_gross_tests;
#[cfg(test)]
mod retained_payout_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod unavailable_ranges_tests;
