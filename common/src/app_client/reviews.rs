//! Review tokens, submitting reviews, host replies and listing reviews.

use super::{get_client, listing_api_audience, listing_api_url};
use crate::models::{
    BookingReviewEligibility, HostReplyRequest, NewReviewRequest, ReviewResponse,
    ReviewTokenInfoResponse,
};
use anyhow::{Context, Result, bail};
use uuid::Uuid;

pub async fn get_review_token_info(token: &str) -> Result<ReviewTokenInfoResponse> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/reviews/token/{}", api_url, token);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to fetch review token info: {}", res.status());
    }

    res.json::<ReviewTokenInfoResponse>()
        .await
        .context("Failed to parse review token info")
}

pub async fn get_booking_review_token(booking_id: Uuid) -> Result<BookingReviewEligibility> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/reviews/booking/{}/token", api_url, booking_id);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to fetch booking review token: {}", res.status());
    }

    res.json::<BookingReviewEligibility>()
        .await
        .context("Failed to parse booking review token")
}

pub async fn submit_review(token: &str, req: &NewReviewRequest) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/reviews/token/{}", api_url, token);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to submit review: {}", res.status());
    }

    Ok(())
}

pub async fn submit_host_reply(review_id: Uuid, req: &HostReplyRequest) -> Result<()> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!("{}/api/v1/reviews/{}/reply", api_url, review_id);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to submit host reply: {}", res.status());
    }

    Ok(())
}

pub async fn get_listing_reviews(
    listing_id: Uuid,
    page: i64,
    per_page: i64,
) -> Result<Vec<ReviewResponse>> {
    let api_url = listing_api_url();
    let audience = listing_api_audience();
    let url = format!(
        "{}/api/v1/listings/{}/reviews?page={}&per_page={}",
        api_url, listing_id, page, per_page
    );

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to listing service")?;

    if !res.status().is_success() {
        bail!("Failed to fetch listing reviews: {}", res.status());
    }

    res.json::<Vec<ReviewResponse>>()
        .await
        .context("Failed to parse listing reviews")
}
