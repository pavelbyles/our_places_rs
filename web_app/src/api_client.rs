use common::http_client::AuthenticatedClient;
use reqwest::Response;
use std::env;
use std::sync::OnceLock;

static CLIENT: OnceLock<AuthenticatedClient> = OnceLock::new();

/// Returns a global instance of the AuthenticatedClient
pub fn get_client() -> &'static AuthenticatedClient {
    CLIENT.get_or_init(|| {
        // Check if running in Cloud Run (var present) or generic prod env
        let is_cloud = env::var("EA__DATABASE__CLOUD").is_ok() || env::var("K_SERVICE").is_ok(); // K_SERVICE is set in Cloud Run
        AuthenticatedClient::new(is_cloud)
    })
}

// Env vars for API URLs
fn listing_api_url() -> String {
    env::var("LISTING_API_URL")
        .unwrap_or_else(|_| "http://localhost:8081".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn booking_api_url() -> String {
    env::var("BOOKING_API_URL")
        .unwrap_or_else(|_| "http://localhost:8082".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn user_api_url() -> String {
    env::var("USER_API_URL")
        .unwrap_or_else(|_| "http://localhost:8083".to_string())
        .trim_end_matches('/')
        .to_string()
}

// Wrapper functions for specific API calls (Example)
pub async fn fetch_listings() -> anyhow::Result<Response> {
    let url = format!("{}/api/listings", listing_api_url());
    // For service-to-service auth, the audience is typically the root URL of the service
    let audience = listing_api_url();

    get_client().get(&url, &audience).await
}

pub async fn fetch_user_profile(user_id: &str) -> anyhow::Result<Response> {
    let url = format!("{}/api/users/{}", user_api_url(), user_id);
    let audience = user_api_url();

    get_client().get(&url, &audience).await
}

pub async fn create_booking<T: serde::Serialize>(booking_data: &T) -> anyhow::Result<Response> {
    let url = format!("{}/api/bookings", booking_api_url());
    let audience = booking_api_url();

    get_client().post(&url, &audience, booking_data).await
}
