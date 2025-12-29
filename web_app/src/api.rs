use crate::models::ListingResponse;
use reqwest;

const LISTING_API_URL: &str = match option_env!("LISTING_API_URL") {
    Some(val) => val,
    None => "http://localhost:8082",
};

pub async fn fetch_listings() -> Result<Vec<ListingResponse>, String> {
    println!("LISTING_API_URL: {}", LISTING_API_URL);
    let url = format!("{}/api/v1/listings/?page=1&per_page=10", LISTING_API_URL);
    reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}
