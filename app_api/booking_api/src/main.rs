//! API for OurPlaces
//!
//! ## Overview
//!
//! Provides a functionality to manage listings and reservations
//!
//! ## List of API's
//!
//! - [x] Create booking

use anyhow::Context;
use api_core::{settings, startup::run, sys};
use booking_api::apis;
use db_core::{connection::create_connection_pool, run_migrations};
use std::net::TcpListener;

/// Test main func doc
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    api_core::tracing_utils::init_subscriber();

    tracing::info!("Starting application");

    // Get settings
    let config = settings::get_settings().context("Could not load settings")?;

    // Create database connection pool
    tracing::info!("Connecting to database");
    let db_connection_pool = create_connection_pool(&config.database.connection_string()).await;
    run_migrations(&db_connection_pool).await;
    tracing::info!(
        "Done connecting to database: {} on {}",
        &config.database.database_name,
        &config.database.host
    );

    // Spawn background cleanup task for stale pending holds
    let cleanup_pool = db_connection_pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(600)); // every 10 mins
        loop {
            interval.tick().await;
            match db_core::booking::cleanup_stale_bookings(&cleanup_pool, 120).await {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!("Cleaned up {} stale bookings", count);
                    }
                }
                Err(e) => tracing::error!("Error cleaning up bookings: {:?}", e),
            }
        }
    });

    // Setup web server
    let http_port: u16 = sys::get_port(config.server.port);
    let address = format!("{}:{}", config.server.host, http_port);

    tracing::info!("Environment is: {}", &config.env);
    tracing::info!("Starting server on port: {}", &http_port);

    let listener = TcpListener::bind(&address)
        .context(format!("Failed to bind to random port {}", address))?;
    let _ = run(
        listener,
        db_connection_pool,
        apis::configure_routes,
        config.clone(),
    )?
    .await
    .context("Server error");

    Ok(())
}
