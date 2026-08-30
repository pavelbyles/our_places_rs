use common::models::{
    BookingReviewEligibility, HostReplyRequest, NewReviewRequest, ReviewResponse,
    ReviewTokenInfoResponse,
};
use leptos::prelude::*;

#[server]
pub async fn get_review_token_info_server(
    token: String,
) -> Result<ReviewTokenInfoResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::get_review_token_info(&token)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = token;
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn get_booking_review_token_server(
    booking_id: uuid::Uuid,
) -> Result<BookingReviewEligibility, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::get_booking_review_token(booking_id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = booking_id;
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn submit_review_server(
    token: String,
    req: NewReviewRequest,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::submit_review(&token, &req)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (token, req);
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn submit_host_reply_server(
    review_id: uuid::Uuid,
    req: HostReplyRequest,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::submit_host_reply(review_id, &req)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (review_id, req);
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn get_listing_reviews_server(
    listing_id: uuid::Uuid,
    page: i64,
    per_page: i64,
) -> Result<Vec<ReviewResponse>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::get_listing_reviews(listing_id, page, per_page)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (listing_id, page, per_page);
        Err(ServerFnError::new("SSR required"))
    }
}
