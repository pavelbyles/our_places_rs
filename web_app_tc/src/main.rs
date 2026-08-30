use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    router::{Router, RouterBuilderDiscoverExt},
};

mod pages;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    // Ensure web_app_common_tc is linked so auto-discovery collects its layout
    let _ = web_app_common_tc::base_layout;

    let mut builder = Router::builder().discover();
    if let Ok(bundle) = AssetBundle::load() {
        builder = builder.assets(bundle);
    }
    let router = builder.build();

    tracing::info!("Starting web_app_tc on 0.0.0.0:3000");
    topcoat::start(router).await?;

    Ok(())
}
