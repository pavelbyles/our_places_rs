//! Listings, price overrides, images and pricing quotes.

use super::{get_client, listing_api_audience, listing_api_url};
use crate::http_client::AuthenticatedClient;
use crate::models::{ListingDetails, ListingResponse, PriceOverride};
use anyhow::{Context, Result, bail};
use rust_decimal::Decimal;
use uuid::Uuid;

/// What to search listings for. Absent fields don't filter.
#[derive(Debug, Default, Clone)]
pub struct ListingSearchParams {
    /// Text the listing name must contain.
    pub name: Option<String>,
    /// Host email address to match, in full or in part.
    pub owner_email: Option<String>,
    /// Optional canonical user ID to query listings belonging strictly to a specific host.
    pub user_id: Option<Uuid>,
    /// Structure types to include.
    pub listing_structure: Option<Vec<String>>,
    /// Highest nightly rate to include.
    pub max_price: Option<Decimal>,
    /// Currency to convert prices into.
    pub currency: Option<String>,
    /// Page number, starting at 1.
    pub page: Option<u32>,
    /// Results per page.
    pub per_page: Option<u32>,
    /// Only this host's listings (their public slug).
    pub host: Option<String>,
    /// Also return inactive listings. Never set this for pages that offer stays.
    pub include_inactive: bool,
    /// Text matched against the listing's name, city or country.
    pub q: Option<String>,
    /// First night of the stay, as the guest gave it. The API validates it.
    pub check_in: Option<String>,
    /// Departure date of the stay, as the guest gave it. The API validates it.
    pub check_out: Option<String>,
    /// Party size, as the guest gave it. The API validates it.
    pub guests: Option<String>,
}

/// A search the API refused, with a message that's safe to show the guest.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Listing search refused: {code}")]
pub struct ListingSearchRefusal {
    /// The API's error code.
    pub code: String,
    /// The message to show.
    pub message: String,
}

/// The refusal in an error response's body, if it has a code whose message is
/// meant for guests. Anything else stays an ordinary error, so its text never
/// reaches a page.
fn search_refusal(body: &str) -> Option<ListingSearchRefusal> {
    let body: serde_json::Value = serde_json::from_str(body).ok()?;
    let code = body["code"].as_str()?;
    let for_guests = matches!(
        code,
        "INVALID_STAY_DATES" | "INVALID_GUESTS" | "INVALID_SEARCH_TEXT"
    ) || code.starts_with("CURRENCY_");
    for_guests.then_some(ListingSearchRefusal {
        code: code.to_string(),
        message: body["message"].as_str()?.to_string(),
    })
}

/// Searches listings through the configured API.
pub async fn search_listings(params: ListingSearchParams) -> Result<Vec<ListingResponse>> {
    search_listings_at(
        get_client(),
        &listing_api_url(),
        &listing_api_audience(),
        params,
    )
    .await
}

/// Searches listings through a given client and address. A refusal the page
/// should show comes back as a [`ListingSearchRefusal`] inside the error.
pub async fn search_listings_at(
    client: &AuthenticatedClient,
    base_url: &str,
    audience: &str,
    params: ListingSearchParams,
) -> Result<Vec<ListingResponse>> {
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(20);
    let base_url = format!("{base_url}/api/v1/listings");
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
        for (key, value) in [
            ("q", params.q),
            ("check_in", params.check_in),
            ("check_out", params.check_out),
            ("guests", params.guests),
        ] {
            if let Some(value) = value.filter(|v| !v.trim().is_empty()) {
                query.append_pair(key, &value);
            }
        }
    }

    let res = client
        .get(url.as_str(), audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        if status.is_client_error()
            && let Some(refusal) = search_refusal(&body)
        {
            return Err(refusal.into());
        }
        bail!("Failed to fetch listings: {status}");
    }

    res.json::<Vec<ListingResponse>>()
        .await
        .context("Failed to deserialize listings response")
}

/// Fetches a listing by ID or slug, with prices in `currency` when given.
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

    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(
            crate::app_client::booking_requests::ApiCallError::from_body(status.as_u16(), &body)
                .into(),
        );
    }

    res.json::<ListingDetails>()
        .await
        .context("Failed to deserialize listing details")
}

/// Creates a listing.
pub async fn create_listing(
    req: &crate::models::NewListingRequest,
    user_jwt: &str,
) -> Result<ListingResponse> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings", api_url);

    let res = get_client()
        .user_request(reqwest::Method::POST, &url, &audience, Some(user_jwt))
        .await?
        .json(req)
        .send()
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

/// Changes a listing.
pub async fn update_listing(
    id: &str,
    req: &crate::models::UpdatedListingRequest,
    user_jwt: &str,
) -> Result<ListingResponse> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}", api_url, id);

    let res = get_client()
        .user_request(reqwest::Method::PATCH, &url, &audience, Some(user_jwt))
        .await?
        .json(req)
        .send()
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

/// Deletes a listing.
pub async fn delete_listing(id: &str, user_jwt: &str) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}", api_url, id);

    let res = get_client()
        .user_request(reqwest::Method::DELETE, &url, &audience, Some(user_jwt))
        .await?
        .send()
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete listing ({}): {}", status, err_text);
    }

    Ok(())
}

/// A listing's price overrides.
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

/// Adds a price override to a listing.
pub async fn create_price_override(
    listing_id: Uuid,
    req: &crate::models::CreatePriceOverrideRequest,
    user_jwt: &str,
) -> Result<PriceOverride> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/listings/{}/price-overrides", api_url, listing_id);

    let res = get_client()
        .user_request(reqwest::Method::POST, &url, &audience, Some(user_jwt))
        .await?
        .json(req)
        .send()
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
pub async fn delete_listing_image(listing_id: Uuid, image_id: Uuid, user_jwt: &str) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!(
        "{}/api/v1/listings/{}/images/{}",
        api_url, listing_id, image_id
    );

    let res = get_client()
        .user_request(reqwest::Method::DELETE, &url, &audience, Some(user_jwt))
        .await?
        .send()
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete listing image ({}): {}", status, err_text);
    }

    Ok(())
}

/// Sets or clears (`None`) a listing photo's name.
pub async fn rename_listing_image(
    listing_id: Uuid,
    image_id: Uuid,
    name: Option<&str>,
    user_jwt: &str,
) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!(
        "{}/api/v1/listings/{}/images/{}",
        api_url, listing_id, image_id
    );

    let res = get_client()
        .user_request(reqwest::Method::PATCH, &url, &audience, Some(user_jwt))
        .await?
        .json(&crate::models::RenameListingImageRequest {
            name: name.map(str::to_string),
        })
        .send()
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to rename listing image ({}): {}", status, err_text);
    }

    Ok(())
}

/// Removes a price override from a listing.
pub async fn delete_price_override(
    listing_id: Uuid,
    override_id: Uuid,
    user_jwt: &str,
) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!(
        "{}/api/v1/listings/{}/price-overrides/{}",
        api_url, listing_id, override_id
    );

    let res = get_client()
        .user_request(reqwest::Method::DELETE, &url, &audience, Some(user_jwt))
        .await?
        .send()
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete price override ({}): {}", status, err_text);
    }

    Ok(())
}
