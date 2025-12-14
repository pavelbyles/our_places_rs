pub mod connection;
pub mod error;
pub mod listing;
pub mod models;
pub mod booking;
pub mod user;

use sqlx::PgPool;
use tracing::info;

pub async fn run_migrations(pool: &PgPool) {
    info!("Running database migrations");
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
    info!("Database migrations complete");
}
