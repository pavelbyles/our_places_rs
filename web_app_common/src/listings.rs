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
        use crate::api_client::get_pool;
        use db_core::listing as db_listing;
        use rust_decimal::Decimal;

        let pool = get_pool().await;
        let listing_details = db_listing::get_listing_by_id(&pool, listing_id)
            .await
            .map_err(|e| ServerFnError::new(format!("Listing not found: {}", e)))?;

        let listing = listing_details.listing;
        let mut base_nightly_rate = listing.price_per_night.unwrap_or(Decimal::ZERO);
        let target_currency = currency.unwrap_or_else(|| listing.base_currency.clone());
        let mut conversion_rate = Decimal::ONE;

        if target_currency != listing.base_currency
            && let Ok((rate, _)) = db_core::currency::get_exchange_rate_and_currency(
                &pool,
                &listing.base_currency,
                &target_currency,
            )
            .await
        {
            conversion_rate = rate;
            base_nightly_rate = (base_nightly_rate * rate).round_dp(2);
        }

        let raw_overrides =
            db_listing::get_active_overrides_for_dates(&pool, listing_id, check_in, check_out)
                .await
                .unwrap_or_default();

        let converted_overrides: Vec<common::models::PriceOverride> = raw_overrides
            .into_iter()
            .map(|mut ovr| {
                if conversion_rate != Decimal::ONE {
                    ovr.nightly_rate = (ovr.nightly_rate * conversion_rate).round_dp(2);
                }
                ovr
            })
            .collect();

        let quote = common::pricing::calculate_dynamic_quote(
            base_nightly_rate,
            listing.minimum_stay,
            &converted_overrides,
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
