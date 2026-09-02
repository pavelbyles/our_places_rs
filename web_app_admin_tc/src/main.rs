use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    cookie::RouterBuilderCookieExt,
    router::{Router, RouterBuilderDiscoverExt},
    session::{RouterBuilderSessionExt, SessionConfig, cookie::CookieTokenStore},
};


use web_app_admin_tc::layout;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    // Default to port 3002 for web_app_admin_tc
    if std::env::var("PORT").is_err() {
        // SAFETY: Called single-threaded at process startup before async runtime work
        unsafe {
            std::env::set_var("PORT", "3002");
        }
    }

    // Link admin layout for Topcoat auto-discovery
    let _ = layout::admin_layout;

    let api_client = web_app_common_tc::TopcoatApiClient::from_env();

    let session_config = SessionConfig::builder()
        .token_store(CookieTokenStore::new().name("op_admin_session"))
        .build();

    let mut builder = Router::builder()
        .discover()
        .cookies()
        .sessions(session_config)
        .app_context(api_client);

    if let Ok(bundle) = AssetBundle::load() {
        builder = builder.assets(bundle);
    }
    let router = builder.build();


    tracing::info!("Starting web_app_admin_tc on 0.0.0.0:3002");
    topcoat::start(router).await?;

    Ok(())
}
