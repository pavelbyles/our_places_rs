//! Server entry point for the redesigned portal.

use std::env;

use tokio::net::TcpListener;
use topcoat::router::{Router, RouterBuilderDiscoverExt};

use web_ui_admin::DEFAULT_PORT;

static HOST_ENV: &str = "HOST";
static PORT_ENV: &str = "PORT";
const DEFAULT_HOST: &str = "127.0.0.1";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    // Link page modules for Topcoat auto-discovery.
    let _ = web_ui_admin::pages::home::home_page;

    let host = env::var(HOST_ENV).unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let port = match env::var(PORT_ENV) {
        Ok(raw) => raw.parse::<u16>()?,
        Err(_) => DEFAULT_PORT,
    };

    let router = Router::builder().discover().build();
    let listener = TcpListener::bind((host.as_str(), port)).await?;
    tracing::info!("Starting web_ui_admin on {host}:{port}");
    topcoat::serve(listener, router).await?;

    Ok(())
}
