/// Legacy path redirects.
pub mod aliases;
/// Sign in and sign out.
pub mod auth;
/// Booking schedule and details.
pub mod bookings;
/// Currency administration card and actions.
pub mod currencies;
/// Dashboard.
pub mod dashboard;
#[cfg(test)]
mod dashboard_tests;
/// One pair's rate history, scheduled-rate deletion and retirement.
pub mod exchange_rate_history;
/// Exchange rate table.
pub mod exchange_rates;
/// Listings studio.
pub mod listings;
/// Payout ledger.
pub mod payouts;
/// Seasonal pricing overrides.
pub mod pricing;
/// Promo code management (#106).
pub mod promo_codes;
/// Host sign-up and verification.
pub mod register;
/// Inbox, request and enquiry detail (#105).
pub mod requests;
/// User administration.
pub mod users;
