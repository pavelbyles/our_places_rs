use leptos::prelude::*;

#[server]
pub async fn listing_search_server(
    name: Option<String>,
    owner_email: Option<String>,
    listing_structure: Option<Vec<String>>,
    max_price: Option<f64>,
    currency: Option<String>,
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

    if let Some(c) = currency {
        url.push_str(&format!("&currency={}", c));
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
    currency: Option<String>,
) -> Result<common::models::ListingDetails, ServerFnError> {
    let api_url = crate::api_client::listing_api_url();
    let mut url = format!("{}/api/v1/listings/{}", api_url, id);
    if let Some(c) = currency {
        url.push_str(&format!("?currency={}", c));
    }

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

#[server]
pub async fn get_pricing_quote_server(
    listing_id: uuid::Uuid,
    check_in: chrono::NaiveDate,
    check_out: chrono::NaiveDate,
    currency: Option<String>,
) -> Result<common::models::DynamicPricingQuote, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use rust_decimal::Decimal;

        // 1. Fetch listing details via listing_api (which applies currency conversion)
        let listing_details =
            get_listing_by_id_server(listing_id.to_string(), currency.clone()).await?;
        let listing = listing_details.listing;
        let base_nightly_rate = listing.price_per_night.unwrap_or(Decimal::ZERO);

        // 2. Fetch price overrides via listing_api
        let api_url = crate::api_client::listing_api_url();
        let audience = crate::api_client::listing_api_audience();
        let url = format!("{}/api/v1/listings/{}/price-overrides", api_url, listing_id);

        let res = crate::api_client::get_client()
            .get(&url, &audience)
            .await
            .map_err(|e| ServerFnError::new(format!("Listing service connection error: {}", e)))?;

        let active_overrides: Vec<common::models::PriceOverride> = if res.status().is_success() {
            let all_overrides: Vec<common::models::PriceOverride> =
                res.json().await.unwrap_or_default();
            all_overrides
                .into_iter()
                .filter(|ovr| ovr.start_date < check_out && ovr.end_date > check_in)
                .collect()
        } else {
            Vec::new()
        };

        // 3. Calculate dynamic quote
        let quote = common::pricing::calculate_dynamic_quote(
            base_nightly_rate,
            listing.minimum_stay,
            &active_overrides,
            check_in,
            check_out,
        )
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        Ok(quote)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (listing_id, check_in, check_out, currency);
        Err(ServerFnError::new("SSR feature required"))
    }
}
