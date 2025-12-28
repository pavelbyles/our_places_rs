use crate::models::ListingResponse;
use reqwest;

pub async fn fetch_listings() -> Result<Vec<ListingResponse>, String> {
    let url = "http://localhost:8082/api/v1/listings/?page=1&per_page=10";
    let client = reqwest::Client::new();
    let res = client.get(url).send().await.map_err(|e| e.to_string())?;
    let listings = res
        .json::<Vec<ListingResponse>>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(listings)
}
