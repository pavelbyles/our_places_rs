//! Listings, price overrides, images and pricing quotes.

use super::{get_client, listing_api_audience, listing_api_url};
use crate::models::{ListingDetails, ListingResponse, PriceOverride};
use anyhow::{Context, Result, bail};
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Debug, Default, Clone)]
pub struct ListingSearchParams {
    pub name: Option<String>,
    pub owner_email: Option<String>,
    /// Optional canonical user ID to query listings belonging strictly to a specific host.
    pub user_id: Option<Uuid>,
    pub listing_structure: Option<Vec<String>>,
    pub max_price: Option<Decimal>,
    pub currency: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    /// Only this host's listings (their public slug).
    pub host: Option<String>,
    /// Also return inactive listings. Never set this for pages that offer stays.
    pub include_inactive: bool,
}

pub async fn search_listings(params: ListingSearchParams) -> Result<Vec<ListingResponse>> {
    let api_url = listing_api_url();
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(20);
    let base_url = format!("{}/api/v1/listings", api_url);
    let mut url =
        reqwest::Url::parse(&base_url).context("Failed to parse listings API base URL")?;

    {
        let mut query = url.query_pairs_mut();
        query.append_pair("page", &page.to_string());
        query.append_pair("per_page", &per_page.to_string());

        if let Some(uid) = params.user_id {
            query.append_pair("user_id", &uid.to_string());
        }
        if let Some(s) = params.name.filter(|s| !s.is_empty()) {
            query.append_pair("name", &s);
        }
        if let Some(h) = params
            .host
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
        {
            query.append_pair("host", h);
        }
        if let Some(s) = params.owner_email.filter(|s| !s.is_empty()) {
            query.append_pair("owner", s.trim());
        }
        if let Some(structures) = params.listing_structure.filter(|s| !s.is_empty()) {
            query.append_pair("structure_type", &structures.join(","));
        }
        if let Some(s) = params.max_price.filter(|s| *s > Decimal::ZERO) {
            query.append_pair("max_price", &s.to_string());
        }
        if let Some(c) = params.currency.filter(|c| !c.is_empty()) {
            query.append_pair("currency", &c);
        }
        if params.include_inactive {
            query.append_pair("include_inactive", "true");
        }
    }

    let url_str = url.to_string();
    let audience = listing_api_audience();
    let res = get_client()
        .get(&url_str, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to fetch listings: {}", res.status());
    }

    res.json::<Vec<ListingResponse>>()
        .await
        .context("Failed to deserialize listings response")
}

pub async fn get_listing_by_id(id: &str, currency: Option<&str>) -> Result<ListingDetails> {
    let api_url = listing_api_url();
    let mut url = format!("{}/api/v1/listings/{}", api_url, id);
    if let Some(c) = currency.filter(|c| !c.is_empty()) {
        url.push_str(&format!("?currency={}", c));
    }

    let audience = listing_api_audience();
    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to fetch listing details: {}", res.status());
    }

    res.json::<ListingDetails>()
        .await
        .context("Failed to deserialize listing details")
}

pub async fn create_listing(req: &crate::models::NewListingRequest) -> Result<ListingResponse> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings", api_url);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to create listing ({}): {}", status, err_text);
    }

    res.json::<ListingResponse>()
        .await
        .context("Failed to parse listing response")
}

pub async fn update_listing(
    id: &str,
    req: &crate::models::UpdatedListingRequest,
) -> Result<ListingResponse> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}", api_url, id);

    let res = get_client()
        .patch(&url, &audience, req)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to update listing ({}): {}", status, err_text);
    }

    res.json::<ListingResponse>()
        .await
        .context("Failed to parse listing response")
}

pub async fn delete_listing(id: &str) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}", api_url, id);

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete listing ({}): {}", status, err_text);
    }

    Ok(())
}

pub async fn get_price_overrides(listing_id: Uuid) -> Result<Vec<PriceOverride>> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}/price-overrides", api_url, listing_id);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        return Ok(Vec::new());
    }

    res.json::<Vec<PriceOverride>>()
        .await
        .context("Failed to deserialize price overrides")
}

pub async fn create_price_override(
    listing_id: Uuid,
    req: &crate::models::CreatePriceOverrideRequest,
) -> Result<PriceOverride> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}/price-overrides", api_url, listing_id);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to create price override ({}): {}", status, err_text);
    }

    res.json::<PriceOverride>()
        .await
        .context("Failed to parse price override response")
}

/// Deletes a listing photo and its resized variants. Missing photos count as deleted.
pub async fn delete_listing_image(listing_id: Uuid, image_id: Uuid) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!(
        "{}/api/v1/listings/{}/images/{}",
        api_url, listing_id, image_id
    );

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete listing image ({}): {}", status, err_text);
    }

    Ok(())
}

pub async fn delete_price_override(listing_id: Uuid, override_id: Uuid) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!(
        "{}/api/v1/listings/{}/price-overrides/{}",
        api_url, listing_id, override_id
    );

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete price override ({}): {}", status, err_text);
    }

    Ok(())
}
