use common::models::{BookingResponse, NewBookingRequest, UpdatedBookingRequest};
use leptos::prelude::*;
use uuid::Uuid;

#[server]
pub async fn create_booking_api(req: NewBookingRequest) -> Result<BookingResponse, ServerFnError> {
    let api_url = crate::api_client::booking_api_url();
    let audience = crate::api_client::booking_api_audience();
    let url = format!("{}/api/v1/bookings", api_url);

    let res = crate::api_client::get_client()
        .post(&url, &audience, &req)
        .await
        .map_err(|e| ServerFnError::new(format!("Booking service connection error: {}", e)))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(ServerFnError::new(format!(
            "Failed to create booking ({}): {}",
            status, err_text
        )));
    }

    let booking: BookingResponse = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to parse booking response: {}", e)))?;

    Ok(booking)
}

#[server]
pub async fn get_booking_by_id_api(
    id: Uuid,
    currency: Option<String>,
) -> Result<BookingResponse, ServerFnError> {
    let api_url = crate::api_client::booking_api_url();
    let audience = crate::api_client::booking_api_audience();
    let mut url = format!("{}/api/v1/bookings/{}", api_url, id);

    if let Some(c) = currency.filter(|c| !c.is_empty()) {
        url.push_str(&format!("?currency={}", c));
    }

    let res = crate::api_client::get_client()
        .get(&url, &audience)
        .await
        .map_err(|e| ServerFnError::new(format!("Booking service connection error: {}", e)))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(ServerFnError::new(format!(
            "Failed to fetch booking details ({}): {}",
            status, err_text
        )));
    }

    let booking: BookingResponse = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to parse booking response: {}", e)))?;

    Ok(booking)
}

#[server]
pub async fn update_booking_api(
    id: Uuid,
    req: UpdatedBookingRequest,
) -> Result<BookingResponse, ServerFnError> {
    let api_url = crate::api_client::booking_api_url();
    let audience = crate::api_client::booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}", api_url, id);

    let res = crate::api_client::get_client()
        .patch(&url, &audience, &req)
        .await
        .map_err(|e| ServerFnError::new(format!("Booking service connection error: {}", e)))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(ServerFnError::new(format!(
            "Failed to update booking ({}): {}",
            status, err_text
        )));
    }

    let booking: BookingResponse = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to parse booking response: {}", e)))?;

    Ok(booking)
}

#[server]
pub async fn delete_booking_api(id: Uuid) -> Result<(), ServerFnError> {
    let api_url = crate::api_client::booking_api_url();
    let audience = crate::api_client::booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}", api_url, id);

    let res = crate::api_client::get_client()
        .delete(&url, &audience)
        .await
        .map_err(|e| ServerFnError::new(format!("Booking service connection error: {}", e)))?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(ServerFnError::new(format!(
            "Failed to delete booking ({}): {}",
            status, err_text
        )));
    }

    Ok(())
}

#[server]
pub async fn transfer_booking_api(
    booking_id: Uuid,
    new_guest_id: Uuid,
) -> Result<BookingResponse, ServerFnError> {
    let api_url = crate::api_client::booking_api_url();
    let audience = crate::api_client::booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}/transfer", api_url, booking_id);

    let req = common::models::TransferBookingRequest {
        guest_id: new_guest_id,
    };

    let res = crate::api_client::get_client()
        .post(&url, &audience, &req)
        .await
        .map_err(|e| ServerFnError::new(format!("Booking service connection error: {}", e)))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(ServerFnError::new(format!(
            "Failed to transfer booking ({}): {}",
            status, err_text
        )));
    }

    let booking: BookingResponse = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to parse booking response: {}", e)))?;

    Ok(booking)
}

#[server]
pub async fn get_user_bookings_api(user_id: Uuid) -> Result<Vec<BookingResponse>, ServerFnError> {
    let api_url = crate::api_client::booking_api_url();
    let audience = crate::api_client::booking_api_audience();
    let url = format!("{}/api/v1/bookings/user/{}", api_url, user_id);

    let res = crate::api_client::get_client()
        .get(&url, &audience)
        .await
        .map_err(|e| ServerFnError::new(format!("Booking service connection error: {}", e)))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(ServerFnError::new(format!(
            "Failed to fetch user bookings ({}): {}",
            status, err_text
        )));
    }

    let bookings: Vec<BookingResponse> = res.json().await.map_err(|e| {
        ServerFnError::new(format!("Failed to parse user bookings response: {}", e))
    })?;

    Ok(bookings)
}

#[server]
pub async fn get_listing_bookings_api(
    listing_id: Uuid,
) -> Result<Vec<BookingResponse>, ServerFnError> {
    let api_url = crate::api_client::booking_api_url();
    let audience = crate::api_client::booking_api_audience();
    let url = format!("{}/api/v1/bookings/listing/{}", api_url, listing_id);

    let res = crate::api_client::get_client()
        .get(&url, &audience)
        .await
        .map_err(|e| ServerFnError::new(format!("Booking service connection error: {}", e)))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(ServerFnError::new(format!(
            "Failed to fetch listing bookings ({}): {}",
            status, err_text
        )));
    }

    let bookings: Vec<BookingResponse> = res.json().await.map_err(|e| {
        ServerFnError::new(format!("Failed to parse listing bookings response: {}", e))
    })?;

    Ok(bookings)
}
