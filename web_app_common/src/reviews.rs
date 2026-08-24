use common::models::{
    BookingReviewEligibility, HostReplyRequest, NewReviewRequest, ReviewResponse,
    ReviewTokenInfoResponse,
};
use leptos::prelude::*;

#[server]
pub async fn get_review_token_info_server(
    token: String,
) -> Result<ReviewTokenInfoResponse, ServerFnError> {
    let api_url = crate::api_client::listing_api_url();
    let audience = crate::api_client::listing_api_audience();
    let url = format!("{}/api/v1/reviews/token/{}", api_url, token);

    let res = crate::api_client::get_client()
        .get(&url, &audience)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !res.status().is_success() {
        return Err(ServerFnError::new(format!(
            "Failed to fetch review token info: {}",
            res.status()
        )));
    }

    let token_info: ReviewTokenInfoResponse = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(token_info)
}

#[server]
pub async fn get_booking_review_token_server(
    booking_id: uuid::Uuid,
) -> Result<BookingReviewEligibility, ServerFnError> {
    let api_url = crate::api_client::listing_api_url();
    let audience = crate::api_client::listing_api_audience();
    let url = format!("{}/api/v1/reviews/booking/{}/token", api_url, booking_id);

    let res = crate::api_client::get_client()
        .get(&url, &audience)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !res.status().is_success() {
        return Err(ServerFnError::new(format!(
            "Failed to fetch booking review token: {}",
            res.status()
        )));
    }

    let eligibility: BookingReviewEligibility = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(eligibility)
}

#[server]
pub async fn submit_review_server(
    token: String,
    req: NewReviewRequest,
) -> Result<(), ServerFnError> {
    let api_url = crate::api_client::listing_api_url();
    let audience = crate::api_client::listing_api_audience();
    let url = format!("{}/api/v1/reviews/token/{}", api_url, token);

    let res = crate::api_client::get_client()
        .post(&url, &audience, &req)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !res.status().is_success() {
        return Err(ServerFnError::new(format!(
            "Failed to submit review: {}",
            res.status()
        )));
    }

    Ok(())
}

#[server]
pub async fn submit_host_reply_server(
    review_id: uuid::Uuid,
    req: HostReplyRequest,
) -> Result<(), ServerFnError> {
    let api_url = crate::api_client::listing_api_url();
    let audience = crate::api_client::listing_api_audience();
    let url = format!("{}/api/v1/reviews/{}/reply", api_url, review_id);

    let res = crate::api_client::get_client()
        .post(&url, &audience, &req)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !res.status().is_success() {
        return Err(ServerFnError::new(format!(
            "Failed to submit host reply: {}",
            res.status()
        )));
    }

    Ok(())
}

#[server]
pub async fn get_listing_reviews_server(
    listing_id: uuid::Uuid,
    page: i64,
    per_page: i64,
) -> Result<Vec<ReviewResponse>, ServerFnError> {
    let api_url = crate::api_client::listing_api_url();
    let audience = crate::api_client::listing_api_audience();
    let url = format!(
        "{}/api/v1/listings/{}/reviews?page={}&per_page={}",
        api_url, listing_id, page, per_page
    );

    let res = crate::api_client::get_client()
        .get(&url, &audience)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !res.status().is_success() {
        return Err(ServerFnError::new(format!(
            "Failed to fetch listing reviews: {}",
            res.status()
        )));
    }

    let reviews: Vec<ReviewResponse> = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(reviews)
}
