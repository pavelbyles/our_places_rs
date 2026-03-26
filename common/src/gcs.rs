use google_cloud_auth::credentials::Builder as AuthBuilder;
use google_cloud_storage::builder::storage::SignedUrlBuilder;
use google_cloud_storage::http::Method;
use std::env;

#[tracing::instrument]
pub async fn generate_v4_signed_url(
    object_path: &str,
    content_type: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let bucket_name =
        env::var("GCS_RAW_BUCKET").unwrap_or_else(|_| "our-places-gcs-img-raw".to_string());

    let signer = AuthBuilder::default().build_signer()?;

    let url =
        SignedUrlBuilder::for_object(format!("projects/_/buckets/{}", bucket_name), object_path)
            .with_method(Method::PUT)
            .with_expiration(std::time::Duration::from_secs(15 * 60))
            .with_header("content-type", content_type)
            .sign_with(&signer)
            .await?;

    Ok(url)
}
