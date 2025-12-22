use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing::info;

pub async fn create_connection_pool(db_url: &str) -> PgPool {
    info!("Creating database connection pool");
    PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to create database connection pool: {:?}", e);
            std::thread::sleep(std::time::Duration::from_secs(2)); // Ensure logs flush
            std::process::exit(1);
        })
}
