use leptos::prelude::*;

#[server]
pub async fn listing_search_server(
    name: Option<String>,
    owner_email: Option<String>,
    listing_structure: Option<Vec<String>>,
    max_price: Option<f64>,
    currency: Option<String>,
) -> Result<Vec<common::models::ListingResponse>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let params = common::app_client::ListingSearchParams {
            name,
            owner_email,
            listing_structure,
            max_price,
            currency,
            page: Some(1),
            per_page: Some(20),
        };
        common::app_client::search_listings(params)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (name, owner_email, listing_structure, max_price, currency);
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn get_listing_by_id_server(
    id: String,
    currency: Option<String>,
) -> Result<common::models::ListingDetails, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::get_listing_by_id(&id, currency.as_deref())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (id, currency);
        Err(ServerFnError::new("SSR required"))
    }
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
        common::app_client::get_pricing_quote(listing_id, check_in, check_out, currency.as_deref())
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (listing_id, check_in, check_out, currency);
        Err(ServerFnError::new("SSR required"))
    }
}
