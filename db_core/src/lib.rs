/// Booking domain entities and operations.
pub mod booking;
/// Guest-to-host booking messaging operations.
pub mod booking_message;
/// Database connection pool initialization and health checks.
pub mod connection;
/// Currency exchange rates and conversions.
pub mod currency;
/// Transactional email outbox storage and lifecycle management.
pub mod email_outbox;
/// Unified database error types.
pub mod error;
/// Listing and property management models and queries.
pub mod listing;
/// Core database entity models and DTOs.
pub mod models;
/// Transactional notification logs and candidate retrieval.
pub mod notification_log;
/// Host financial earnings and payout ledger.
pub mod payout_ledger;
/// Guest reviews and verification tokens.
pub mod review;
/// Session store and authentication token management.
pub mod sessions;
/// User account and profile operations.
pub mod user;

pub use sqlx::PgPool;
use tracing::info;

pub async fn run_migrations(pool: &PgPool) {
    info!("Running database migrations");
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
    info!("Database migrations complete");
}
