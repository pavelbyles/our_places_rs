//! Enquiries and requests to book (#105): creation, host responses,
//! conversion to a `pending` hold, expiry and the inbox.
//!
//! Every transaction that changes whether dates are blocked takes the listing
//! row lock first, the same lock holds take, then (in order) request rows,
//! currency, the guest's pending bookings and a promo code. A request blocks
//! its dates while `status IN ('open','accepted','quoted') AND expires_at >
//! NOW()`, so expiry never waits for a sweep.

mod convert;
mod history;
mod queries;
mod request;
mod respond;
mod supersede;

pub use convert::{Converted, OfferCheckout, convert_offer_to_hold};
pub use history::{Actor, HistorySubject};
pub use queries::{RequestDetail, get_request, list_for_guest, quotes_by_id};
pub use request::{CreatedRequest, NewRequest, create_request};
pub use respond::{accept_request, decline_request, withdraw_request};
pub use supersede::Superseded;
pub(crate) use supersede::supersede_for_instant_hold;

#[cfg(test)]
mod convert_tests;
#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod hold_request_tests;
#[cfg(test)]
mod promo_request_tests;
#[cfg(test)]
mod request_tests;
#[cfg(test)]
mod respond_tests;
