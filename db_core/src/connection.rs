use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing::info;

/// Connects to the database with the service's pool settings.
pub async fn create_connection_pool(db_url: &str) -> PgPool {
    info!("Creating database connection pool");
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(60))
        .connect(db_url)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to create database connection pool: {:?}", e);
            std::thread::sleep(std::time::Duration::from_secs(2)); // Ensure logs flush
            std::process::exit(1);
        })
}

/// Connects with `max_connections`, returning the error instead of exiting,
/// for the calendar sync job (its pool is `concurrency + 2`). Runs no
/// migrations.
pub async fn create_connection_pool_sized(
    db_url: &str,
    max_connections: u32,
) -> Result<PgPool, sqlx::Error> {
    info!(max_connections, "Creating sized database connection pool");
    PgPoolOptions::new()
        .max_connections(max_connections.max(1))
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(db_url)
        .await
}
