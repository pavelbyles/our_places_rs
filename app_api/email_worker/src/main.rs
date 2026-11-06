//! Email worker: delivers queued emails on Pub/Sub events.
use anyhow::Context;
use api_core::{settings, startup::run, sys};
use db_core::{connection::create_connection_pool, run_migrations};
use std::net::TcpListener;

mod apis;
/// Worker configuration.
pub mod config;
/// Turns Pub/Sub events into sent emails.
pub mod processor;
/// Email delivery providers.
pub mod provider;
/// Enquiry and request-to-book emails.
mod request_emails;
#[cfg(test)]
mod tests;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Initialize tracing
    api_core::tracing_utils::init_subscriber(env!("CARGO_PKG_NAME"));

    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing::info!("Starting email_worker service");

    // Get settings
    let config = settings::get_settings().context("Could not load settings")?;

    // Create database connection pool
    tracing::info!(
        "Connecting to database: {}",
        &config.database.connection_string()
    );
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
    tracing::info!("Starting email_worker server on port: {}", &http_port);

    let listener =
        TcpListener::bind(&address).context(format!("Failed to bind to address {}", address))?;
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
