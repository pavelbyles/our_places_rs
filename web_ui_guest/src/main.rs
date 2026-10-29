//! Server entry point for the redesigned guest portal.

use std::env;

use tokio::net::TcpListener;
use topcoat::asset::{AssetBundle, RouterBuilderAssetExt};
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::router::{Router, RouterBuilderDiscoverExt};
use topcoat::session::{RouterBuilderSessionExt, SessionConfig, cookie::CookieTokenStore};
use web_client::{TopcoatApiClient, jwt::ServiceJwt};
use web_ui_common::security_headers::SecurityHeaders;

use web_ui_guest::DEFAULT_PORT;

static HOST_ENV: &str = "HOST";
static PORT_ENV: &str = "PORT";
static HSTS_ENV: &str = "ENABLE_HSTS";
const DEFAULT_HOST: &str = "127.0.0.1";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    // Link the layout so Topcoat's discovery sees this crate's routes.
    let _ = web_ui_guest::layout::guest_layout;

    // Fail fast: no fallback signing secret, and rendering any page panics
    // without the asset bundle (topcoat-asset), so neither may be missing.
    let service_jwt = ServiceJwt::from_env()?;
    let assets = AssetBundle::load()?;

    let host = env::var(HOST_ENV).unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let port = match env::var(PORT_ENV) {
        Ok(raw) => raw.parse::<u16>()?,
        Err(_) => DEFAULT_PORT,
    };
    let hsts = env::var(HSTS_ENV).is_ok_and(|v| v == "1" || v == "true");

    let sessions = SessionConfig::builder()
        .token_store(CookieTokenStore::new().name("op_guest_session"))
        .build();

    // No `.runtime()`: it needs 'unsafe-eval' (spec, Phase 0 spike results).
    let router = Router::builder()
        .discover()
        .cookies()
        .sessions(sessions)
        .app_context(TopcoatApiClient::from_env())
        .app_context(service_jwt)
        .assets(assets)
        .layer(
            SecurityHeaders::new()
                .with_hsts(hsts)
                .with_storage_origin(common::gcs::emulator_endpoint()),
        )
        .build();

    let listener = TcpListener::bind((host.as_str(), port)).await?;
    tracing::info!("Starting web_ui_guest on {host}:{port}");
    topcoat::serve(listener, router).await?;
    Ok(())
}
