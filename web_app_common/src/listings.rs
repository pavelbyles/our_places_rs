use leptos::prelude::*;

#[server]
pub async fn listing_search_server(
    name: Option<String>,
    owner_email: Option<String>,
    listing_structure: Option<Vec<String>>,
    max_price: Option<f64>,
) -> Result<Vec<common::models::ListingResponse>, ServerFnError> {
    let api_url = crate::api_client::listing_api_url();
    let mut url = format!("{}/api/v1/listings?page=1&per_page=20", api_url);

    if let Some(s) = name.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&name={}", s));
    }

    if let Some(s) = owner_email.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&owner={}", s));
    }

    if let Some(structures) = listing_structure.filter(|s| !s.is_empty()) {
        let joined = structures.join(",");
        url.push_str(&format!("&structure_type={}", joined));
    }

    if let Some(s) = max_price.filter(|&s| s > 0.0) {
        url.push_str(&format!("&max_price={}", s));
    }

    let res = crate::api_client::get_client()
        .get(&url, &api_url)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !res.status().is_success() {
        return Err(ServerFnError::new(format!(
            "Failed to fetch listings: {}",
            res.status()
        )));
    }

    let listings: Vec<common::models::ListingResponse> = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(listings)
}

#[server]
pub async fn get_listing_by_id_server(
    id: String,
) -> Result<common::models::ListingDetails, ServerFnError> {
    let api_url = crate::api_client::listing_api_url();
    let url = format!("{}/api/v1/listings/{}", api_url, id);

    let res = crate::api_client::get_client()
        .get(&url, &api_url)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !res.status().is_success() {
        return Err(ServerFnError::new(format!(
            "Failed to fetch listing details: {}",
            res.status()
        )));
    }

    let details: common::models::ListingDetails = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(details)
}
