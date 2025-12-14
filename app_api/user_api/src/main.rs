//! API for OurPlaces
//!
//! ## Overview
//!
//! Provides a functionality to manage listings and reservations
//!
//! ## List of API's
//!
//! - [x] Create user

use anyhow::Context;
use api_core::{
    startup::run,
    settings,
    sys,
};
use db_core::{
    connection::create_connection_pool,
    run_migrations,
};
use std::net::TcpListener;
use tracing_subscriber::{
    layer::SubscriberExt, 
    util::SubscriberInitExt, 
    fmt::format::FmtSpan,
};
mod apis;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::CLOSE))
        .init();

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

    // Setup web server
    let http_port: u16 = sys::get_port(config.server.port);
    let address = format!("{}:{}", config.server.host, http_port);

    tracing::info!("Environment is: {}", &config.env);
    tracing::info!("Starting server on port: {}", &http_port);

    let listener = TcpListener::bind(&address)
        .context(format!("Failed to bind to random port {}", address))?;
    let _ = run(listener, db_connection_pool, apis::configure_routes, config.clone())?
        .await
        .context("Server error");

    Ok(())
}