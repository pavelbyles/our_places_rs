//! Calendar sync IO shared by `listing_api` and the sync job (#58).

/// The SSRF-guarded feed fetcher.
pub mod fetch;
/// Syncing claimed feeds.
pub mod sync;

pub use fetch::{FetchError, FetcherConfig, GuardedFetcher};

use crate::settings::CalendarSyncSettings;

/// Builds the fetcher config for `settings`, reading the extra root CA if one
/// is set. Production settings give [`FetcherConfig::default`].
pub async fn fetcher_config(settings: &CalendarSyncSettings) -> Result<FetcherConfig, FetchError> {
    let extra_root_ca = match &settings.extra_root_ca_path {
        None => None,
        Some(path) => {
            let pem = tokio::fs::read(path).await.map_err(|e| {
                tracing::error!(error = %e, "calendar extra root CA unreadable");
                FetchError::Client
            })?;
            Some(reqwest::Certificate::from_pem(&pem).map_err(|e| {
                tracing::error!(error = %e.without_url(), "calendar extra root CA invalid");
                FetchError::Client
            })?)
        }
    };
    Ok(FetcherConfig {
        allow_private_hosts: settings.allow_private_hosts,
        extra_root_ca,
        ..FetcherConfig::default()
    })
}

#[cfg(test)]
mod fetch_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod sync_tests;
/// A local https feed server and fixed resolver, for tests.
#[cfg(any(test, feature = "test-support"))]
pub mod test_server;
