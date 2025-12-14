use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing::info;

pub async fn create_connection_pool(db_url: &str) -> PgPool {
    info!("Creating database connection pool");
    PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("Failed to create database connection pool")
}
