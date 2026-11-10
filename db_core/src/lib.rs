//! PostgreSQL schema access: queries, row locks and migrations.
#![deny(missing_docs)]

/// Booking domain entities and operations.
pub mod booking;
/// Guest-to-host booking messaging operations.
pub mod booking_message;
/// Enquiries and requests to book: creation, responses, conversion and expiry.
pub mod booking_request;
/// iCal calendar sync: feeds, blocked dates, conflicts and export.
pub mod calendar;
/// Database connection pool initialization and health checks.
pub mod connection;
/// Guest × listing conversations and their messages.
pub mod conversation;
#[cfg(test)]
mod conversation_tests;
/// Currency exchange rates and conversions.
pub mod currency;
/// Transactional email outbox storage and lifecycle management.
pub mod email_outbox;
/// Unified database error types.
pub mod error;
/// Foreign exchange rate queries with direct and inverse resolution.
pub mod exchange_rate;
/// Listing and property management models and queries.
pub mod listing;
/// Core database entity models and DTOs.
pub mod models;
/// Transactional notification logs and candidate retrieval.
pub mod notification_log;
/// Host financial earnings and payout ledger.
pub mod payout_ledger;
/// Promo codes: lookups, eligibility and usage.
pub mod promo;
#[cfg(test)]
mod promo_tests;
#[cfg(test)]
mod request_migration_tests;
/// Bilateral reservation change requests and lifecycle operations.
pub mod reservation_change;
/// Guest reviews and verification tokens.
pub mod review;
/// Session store and authentication token management.
pub mod sessions;
/// Statutory tax documents and compliance invoicing.
pub mod tax_document;
/// User account and profile operations.
pub mod user;

pub use sqlx::PgPool;
use tracing::info;

/// Applies pending migrations, and stops the service if they fail.
pub async fn run_migrations(pool: &PgPool) {
    info!("Running database migrations");
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
    info!("Database migrations complete");
}

/// Env var holding the test database URL (tests only).
#[cfg(test)]
pub(crate) const DATABASE_URL_ENV: &str = "DATABASE_URL";
