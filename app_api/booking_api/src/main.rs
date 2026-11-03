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
    dotenvy::dotenv().ok();

    // Initialize tracing
    api_core::tracing_utils::init_subscriber(env!("CARGO_PKG_NAME"));

    tracing::info!("Starting application");

    // Get settings
    let config = settings::get_settings().context("Could not load settings")?;

    // User tokens cannot be verified without it, and there is no fallback.
    api_core::auth::jwt_secret().context("Refusing to start")?;

    if config.application.pending_hold_minutes <= 0 {
        anyhow::bail!(
            "application.pending_hold_minutes must be positive, got {}",
            config.application.pending_hold_minutes
        );
    }
    config
        .application
        .validate_max_stay_nights()
        .map_err(anyhow::Error::msg)
        .context("Refusing to start")?;
    tracing::info!(
        hold_minutes = config.application.pending_hold_minutes,
        "Pending booking hold window"
    );
    let grace = config
        .application
        .rate_effective_grace()
        .map_err(anyhow::Error::msg)
        .context("Refusing to start")?;
    tracing::info!(
        grace_seconds = grace.num_seconds(),
        "Exchange-rate grace window"
    );

    // Validate statutory tax configuration fast-fail (Spec §1.7)
    let _tax_settings =
        booking_api::settings::TaxSettings::from_env().map_err(|e| anyhow::anyhow!(e))?;

    // Validate cron secret configuration fast-fail (Spec §1.7)
    let _cron_settings =
        booking_api::settings::CronSettings::from_env().map_err(|e| anyhow::anyhow!(e))?;

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
    let served = run(
        listener,
        db_connection_pool,
        apis::configure_routes,
        config.clone(),
    )?
    .await
    .context("Server error");

    // Send any buffered spans before the instance goes away.
    api_core::tracing_utils::shutdown_telemetry().await;
    served
}
