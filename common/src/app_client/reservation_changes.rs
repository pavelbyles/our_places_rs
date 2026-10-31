//! Reservation change requests: quote, create, list, approve, decline, cancel.

use super::*;

fn extract_api_error_message(err_text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(err_text)
        .ok()
        .and_then(|v| {
            v.get("message")
                .or_else(|| v.get("error"))
                .and_then(|m| m.as_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| err_text.to_string())
}

/// Requests a live pricing & payout quote for a proposed reservation change.
pub async fn quote_reservation_change(
    booking_id: Uuid,
    body: &crate::models::ReservationChangeQuoteRequest,
    user_jwt: Option<&str>,
) -> Result<crate::models::ReservationChangeQuoteResponse> {
    let api_url = booking_api_url();
    let url = format!("{}/api/v1/bookings/{}/change-quote", api_url, booking_id);
    let req = get_client()
        .user_request(
            reqwest::Method::POST,
            &url,
            &booking_api_audience(),
            user_jwt,
        )
        .await?
        .json(body);
    let res = req
        .send()
        .await
        .context("Failed to connect to booking service")?;
    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        bail!("{}", extract_api_error_message(&err_text));
    }
    res.json::<crate::models::ReservationChangeQuoteResponse>()
        .await
        .context("Failed to parse reservation change quote response")
}

/// Submits a bilateral reservation change request awaiting counterparty approval.
pub async fn create_reservation_change_request(
    booking_id: Uuid,
    body: &crate::models::CreateReservationChangeRequest,
    user_jwt: Option<&str>,
) -> Result<crate::models::ReservationChangeRequestResponse> {
    let api_url = booking_api_url();
    let url = format!("{}/api/v1/bookings/{}/change-requests", api_url, booking_id);
    let req = get_client()
        .user_request(
            reqwest::Method::POST,
            &url,
            &booking_api_audience(),
            user_jwt,
        )
        .await?
        .json(body);
    let res = req
        .send()
        .await
        .context("Failed to connect to booking service")?;
    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        bail!("{}", extract_api_error_message(&err_text));
    }
    res.json::<crate::models::ReservationChangeRequestResponse>()
        .await
        .context("Failed to parse reservation change request response")
}

/// Lists all reservation change requests for a booking.
pub async fn list_reservation_change_requests(
    booking_id: Uuid,
    user_jwt: Option<&str>,
) -> Result<Vec<crate::models::ReservationChangeRequestResponse>> {
    let api_url = booking_api_url();
    let url = format!("{}/api/v1/bookings/{}/change-requests", api_url, booking_id);
    let req = get_client()
        .user_request(
            reqwest::Method::GET,
            &url,
            &booking_api_audience(),
            user_jwt,
        )
        .await?;
    let res = req
        .send()
        .await
        .context("Failed to connect to booking service")?;
    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        bail!("{}", extract_api_error_message(&err_text));
    }
    res.json::<Vec<crate::models::ReservationChangeRequestResponse>>()
        .await
        .context("Failed to parse reservation change requests list")
}

/// Approves a pending reservation change request as the required counterparty.
pub async fn approve_reservation_change_request(
    booking_id: Uuid,
    request_id: Uuid,
    body: &crate::models::RespondReservationChangeRequest,
    user_jwt: Option<&str>,
) -> Result<crate::models::ReservationChangeRequestResponse> {
    let api_url = booking_api_url();
    let url = format!(
        "{}/api/v1/bookings/{}/change-requests/{}/approve",
        api_url, booking_id, request_id
    );
    let req = get_client()
        .user_request(
            reqwest::Method::POST,
            &url,
            &booking_api_audience(),
            user_jwt,
        )
        .await?
        .json(body);
    let res = req
        .send()
        .await
        .context("Failed to connect to booking service")?;
    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        bail!("{}", extract_api_error_message(&err_text));
    }
    res.json::<crate::models::ReservationChangeRequestResponse>()
        .await
        .context("Failed to parse approved reservation change response")
}

/// Declines a pending reservation change request as the required counterparty.
pub async fn decline_reservation_change_request(
    booking_id: Uuid,
    request_id: Uuid,
    body: &crate::models::RespondReservationChangeRequest,
    user_jwt: Option<&str>,
) -> Result<crate::models::ReservationChangeRequestResponse> {
    let api_url = booking_api_url();
    let url = format!(
        "{}/api/v1/bookings/{}/change-requests/{}/decline",
        api_url, booking_id, request_id
    );
    let req = get_client()
        .user_request(
            reqwest::Method::POST,
            &url,
            &booking_api_audience(),
            user_jwt,
        )
        .await?
        .json(body);
    let res = req
        .send()
        .await
        .context("Failed to connect to booking service")?;
    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        bail!("{}", extract_api_error_message(&err_text));
    }
    res.json::<crate::models::ReservationChangeRequestResponse>()
        .await
        .context("Failed to parse declined reservation change response")
}

/// Withdraws (cancels) a pending reservation change request as the initiator.
pub async fn cancel_reservation_change_request(
    booking_id: Uuid,
    request_id: Uuid,
    user_jwt: Option<&str>,
) -> Result<crate::models::ReservationChangeRequestResponse> {
    let api_url = booking_api_url();
    let url = format!(
        "{}/api/v1/bookings/{}/change-requests/{}/cancel",
        api_url, booking_id, request_id
    );
    let req = get_client()
        .user_request(
            reqwest::Method::POST,
            &url,
            &booking_api_audience(),
            user_jwt,
        )
        .await?;
    let res = req
        .send()
        .await
        .context("Failed to connect to booking service")?;
    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        bail!("{}", extract_api_error_message(&err_text));
    }
    res.json::<crate::models::ReservationChangeRequestResponse>()
        .await
        .context("Failed to parse cancelled reservation change response")
}
