//! API for OurPlaces
//!
//! ## Overview
//!
//! Provides a functionality to manage listings and reservations
//!
//! ## List of API's
//!
//! - [x] Create listings
//! - [x] Get listings
//! - [x] Get listing by id
//! - [x] Update listings
//! - [x] Delete listings

use anyhow::Context;
use api_core::{settings, startup::run, sys};
use db_core::{connection::create_connection_pool, run_migrations};
use listing_api::apis;
use std::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Initialize tracing
    api_core::tracing_utils::init_subscriber(env!("CARGO_PKG_NAME"));

    tracing::info!("Starting application");

    // Get settings
    let config = settings::get_settings().context("Could not load settings")?;

    config
        .application
        .validate_max_stay_nights()
        .map_err(anyhow::Error::msg)
        .context("Refusing to start")?;

    // User tokens cannot be verified without it, and there is no fallback.
    api_core::auth::jwt_secret().context("Refusing to start")?;

    // Calendar sync settings come from the environment only. Local-only
    // toggles stop the service on Cloud Run.
    let calendar_settings = settings::CalendarSyncSettings::from_process_env()
        .map_err(|e| {
            tracing::error!(error = %e, "invalid calendar sync settings");
            anyhow::Error::msg(e)
        })
        .context("Refusing to start")?;
    settings::check_cloud_guard(&calendar_settings, |key| std::env::var(key).ok())
        .map_err(|e| {
            tracing::error!(error = %e, "calendar sync settings refused on Cloud Run");
            anyhow::Error::msg(e)
        })
        .context("Refusing to start")?;
    let admin_base_url = std::env::var(listing_api::calendar::ADMIN_BASE_URL_ENV)
        .unwrap_or_else(|_| "http://localhost:3012".to_string());
    let calendar = actix_web::web::Data::new(listing_api::calendar::CalendarRuntime::new(
        calendar_settings,
        std::sync::Arc::new(api_core::email_publisher::EmailPublisher::from_env()),
        admin_base_url,
    ));

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
        move |cfg: &mut actix_web::web::ServiceConfig| {
            cfg.app_data(calendar.clone());
            apis::configure_routes(cfg);
        },
        config.clone(),
    )?
    .await
    .context("Server error");

    // Send any buffered spans before the instance goes away.
    api_core::tracing_utils::shutdown_telemetry().await;
    served
}
