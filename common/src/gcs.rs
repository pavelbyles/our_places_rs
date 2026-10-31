use google_cloud_auth::credentials::Builder as AuthBuilder;
use google_cloud_auth::credentials::anonymous::Builder as AnonymousBuilder;
use google_cloud_storage::builder::storage::SignedUrlBuilder;
use google_cloud_storage::client::Storage;
use google_cloud_storage::http::Method;
use std::env;

/// Cloud Storage's public endpoint, used whenever no emulator is configured.
pub const GCS_PUBLIC_ENDPOINT: &str = "https://storage.googleapis.com";

/// Env var naming a local Cloud Storage emulator (fake-gcs-server), e.g.
/// `localhost:4443` or `http://localhost:4443`. Same convention as the
/// official client libraries.
pub const STORAGE_EMULATOR_HOST_ENV: &str = "STORAGE_EMULATOR_HOST";

/// Turns a `STORAGE_EMULATOR_HOST` value into a base URL: blank values are
/// ignored, a missing scheme defaults to `http://`, trailing slashes go.
pub fn emulator_endpoint_from(raw: &str) -> Option<String> {
    Some(raw.trim().trim_end_matches('/'))
        .filter(|host| !host.is_empty())
        .map(|host| {
            if host.contains("://") {
                host.to_string()
            } else {
                format!("http://{host}")
            }
        })
}

/// The local Cloud Storage emulator's base URL, if one is configured.
pub fn emulator_endpoint() -> Option<String> {
    env::var(STORAGE_EMULATOR_HOST_ENV)
        .ok()
        .and_then(|raw| emulator_endpoint_from(&raw))
}

/// Public URL of an object, served by `endpoint` (path style).
pub fn object_url(endpoint: &str, bucket: &str, object: &str) -> String {
    format!("{endpoint}/{bucket}/{object}")
}

/// Public URL of an object: the emulator's when one is configured,
/// otherwise Cloud Storage's.
pub fn public_object_url(bucket: &str, object: &str) -> String {
    let endpoint = emulator_endpoint().unwrap_or_else(|| GCS_PUBLIC_ENDPOINT.to_string());
    object_url(&endpoint, bucket, object)
}

/// A Cloud Storage client. Against the emulator it uses anonymous
/// credentials, so local development needs no Google Cloud account.
pub async fn storage_client() -> Result<Storage, Box<dyn std::error::Error>> {
    let client = match emulator_endpoint() {
        Some(endpoint) => {
            Storage::builder()
                .with_endpoint(endpoint)
                .with_credentials(AnonymousBuilder::new().build())
                .build()
                .await?
        }
        None => Storage::builder().build().await?,
    };
    Ok(client)
}

#[tracing::instrument]
pub async fn generate_v4_signed_url(
    object_path: &str,
    content_type: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let bucket_name =
        env::var("GCS_RAW_BUCKET").unwrap_or_else(|_| "our-places-gcs-img-raw".to_string());

    let signer = AuthBuilder::default().build_signer()?;

    let builder =
        SignedUrlBuilder::for_object(format!("projects/_/buckets/{}", bucket_name), object_path)
            .with_method(Method::PUT)
            .with_expiration(std::time::Duration::from_secs(15 * 60))
            .with_header("content-type", content_type);
    // The emulator accepts signed-URL uploads but does not verify signatures.
    let builder = match emulator_endpoint() {
        Some(endpoint) => builder.with_endpoint(endpoint),
        None => builder,
    };
    let url = builder.sign_with(&signer).await?;

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emulator_host_without_scheme_defaults_to_http() {
        assert_eq!(
            emulator_endpoint_from("localhost:4443").as_deref(),
            Some("http://localhost:4443")
        );
    }

    #[test]
    fn emulator_host_keeps_its_scheme_and_drops_trailing_slashes() {
        assert_eq!(
            emulator_endpoint_from(" https://gcs.local:4443/ ").as_deref(),
            Some("https://gcs.local:4443")
        );
    }

    #[test]
    fn blank_emulator_host_means_no_emulator() {
        assert_eq!(emulator_endpoint_from("  "), None);
    }

    #[test]
    fn object_urls_are_path_style() {
        assert_eq!(
            object_url(GCS_PUBLIC_ENDPOINT, "bucket", "optimized/mobile/a.webp"),
            "https://storage.googleapis.com/bucket/optimized/mobile/a.webp"
        );
    }
}
