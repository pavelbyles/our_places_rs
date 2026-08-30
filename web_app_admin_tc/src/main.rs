use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    router::{Router, RouterBuilderDiscoverExt},
};

mod pages;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let mut builder = Router::builder()
        .discover()
        .layout(web_app_common_tc::base_layout);
    if let Ok(bundle) = AssetBundle::load() {
        builder = builder.assets(bundle);
    }
    let router = builder.build();

    tracing::info!("Starting web_app_admin_tc on 0.0.0.0:3002");
    topcoat::start(router).await?;

    Ok(())
}
