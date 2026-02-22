use anyhow::Context;
use api_core::{settings, startup::run, sys};
use db_core::{connection::create_connection_pool, run_migrations};
use std::net::TcpListener;

mod apis;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    api_core::tracing_utils::init_subscriber();

    tracing::info!("Starting application");

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
