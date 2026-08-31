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

    let mut builder = Router::builder().discover();
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
