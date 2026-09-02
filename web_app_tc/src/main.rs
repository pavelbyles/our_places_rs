use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    router::{Router, RouterBuilderDiscoverExt},
};

mod layout;
mod pages;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    // Link guest layout for Topcoat auto-discovery
    let _ = layout::guest_layout;

    let api_client = web_app_common_tc::TopcoatApiClient::from_env();

    let mut builder = Router::builder().discover().app_context(api_client);

    match AssetBundle::load() {
        Ok(bundle) => {
            builder = builder.assets(bundle);
        }
        Err(e) => {
            tracing::error!("Failed to load assets: {}", e);
        }
    }
    let router = builder.build();

    tracing::info!("Starting web_app_tc on 0.0.0.0:3000");
    topcoat::start(router).await?;

    Ok(())
}
