//! Bookings, cancellation quotes, messages and tax documents.

use super::*;

/// Places a hold as the user who owns `user_jwt` (must be `req.guest_id`).
pub async fn create_booking(req: &NewBookingRequest, user_jwt: &str) -> Result<BookingResponse> {
    let api_url = booking_api_url();
    let url = format!("{}/api/v1/bookings", api_url);

    let res = get_client()
        .user_request(
            reqwest::Method::POST,
            &url,
            &booking_api_audience(),
            Some(user_jwt),
        )
        .await?
        .json(req)
        .send()
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to create booking ({}): {}", status, err_text);
    }

    res.json::<BookingResponse>()
        .await
        .context("Failed to parse booking response")
}

pub async fn get_booking_by_id(id: Uuid, currency: Option<&str>) -> Result<BookingResponse> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let mut url = format!("{}/api/v1/bookings/{}", api_url, id);
    if let Some(c) = currency.filter(|c| !c.is_empty()) {
        url.push_str(&format!("?currency={}", c));
    }

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch booking details ({}): {}", status, err_text);
    }

    res.json::<BookingResponse>()
        .await
        .context("Failed to parse booking response")
}

pub async fn update_booking(id: Uuid, req: &UpdatedBookingRequest) -> Result<BookingResponse> {
    update_booking_with_auth(id, req, None).await
}

pub async fn update_booking_with_auth(
    id: Uuid,
    req: &UpdatedBookingRequest,
    user_jwt: Option<&str>,
) -> Result<BookingResponse> {
    let api_url = booking_api_url();
    let url = format!("{}/api/v1/bookings/{}", api_url, id);

    let request = get_client()
        .user_request(
            reqwest::Method::PATCH,
            &url,
            &booking_api_audience(),
            user_jwt,
        )
        .await?
        .json(req);

    let res = request
        .send()
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to update booking ({}): {}", status, err_text);
    }

    res.json::<BookingResponse>()
        .await
        .context("Failed to parse booking response")
}

pub async fn get_cancellation_quote(id: Uuid) -> Result<crate::cancellation::RefundQuote> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}/cancellation-quote", api_url, id);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!(
            "Failed to get cancellation quote ({}): {}",
            status,
            err_text
        );
    }

    res.json::<crate::cancellation::RefundQuote>()
        .await
        .context("Failed to parse cancellation quote response")
}

pub async fn delete_booking(id: Uuid) -> Result<()> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}", api_url, id);

    let res = get_client()
        .delete(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to delete booking ({}): {}", status, err_text);
    }

    Ok(())
}

pub async fn transfer_booking(id: Uuid, new_guest_id: Uuid) -> Result<BookingResponse> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}/transfer", api_url, id);

    let req = TransferBookingRequest {
        guest_id: new_guest_id,
    };

    let res = get_client()
        .post(&url, &audience, &req)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to transfer booking ({}): {}", status, err_text);
    }

    res.json::<BookingResponse>()
        .await
        .context("Failed to parse booking response")
}

pub async fn get_user_bookings(user_id: Uuid) -> Result<Vec<BookingResponse>> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/user/{}", api_url, user_id);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch user bookings ({}): {}", status, err_text);
    }

    res.json::<Vec<BookingResponse>>()
        .await
        .context("Failed to parse user bookings response")
}

pub async fn get_listing_bookings(listing_id: Uuid) -> Result<Vec<BookingResponse>> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/listing/{}", api_url, listing_id);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!(
            "Failed to fetch listing bookings ({}): {}",
            status,
            err_text
        );
    }

    res.json::<Vec<BookingResponse>>()
        .await
        .context("Failed to parse listing bookings response")
}

/// Bookings visible to the user who owns `user_jwt`: every booking for an
/// admin, bookings on their own listings for a host.
pub async fn get_all_bookings(
    page: Option<u32>,
    per_page: Option<u32>,
    user_jwt: &str,
) -> Result<Vec<BookingResponse>> {
    let api_url = booking_api_url();
    let page = page.unwrap_or(1);
    let per_page = per_page.unwrap_or(50);
    let url = format!(
        "{}/api/v1/bookings?page={}&per_page={}",
        api_url, page, per_page
    );

    let res = get_client()
        .user_request(
            reqwest::Method::GET,
            &url,
            &booking_api_audience(),
            Some(user_jwt),
        )
        .await?
        .send()
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch bookings ({}): {}", status, err_text);
    }

    res.json::<Vec<BookingResponse>>()
        .await
        .context("Failed to parse bookings response")
}

pub async fn get_booking_messages(id: Uuid) -> Result<BookingMessagesWrapper> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}/messages", api_url, id);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!(
            "Failed to fetch booking messages ({}): {}",
            status,
            err_text
        );
    }

    res.json::<BookingMessagesWrapper>()
        .await
        .context("Failed to parse booking messages response")
}

pub async fn send_booking_message(
    id: Uuid,
    req: &CreateBookingMessageRequest,
) -> Result<BookingMessageResponse> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}/messages", api_url, id);

    let res = get_client()
        .post(&url, &audience, req)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to send booking message ({}): {}", status, err_text);
    }

    res.json::<BookingMessageResponse>()
        .await
        .context("Failed to parse booking message response")
}

pub async fn mark_booking_messages_read(id: Uuid) -> Result<MarkMessagesReadResponse> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/bookings/{}/messages/read", api_url, id);

    let res = get_client()
        .patch(&url, &audience, &serde_json::json!({}))
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!(
            "Failed to mark booking messages as read ({}): {}",
            status,
            err_text
        );
    }

    res.json::<MarkMessagesReadResponse>()
        .await
        .context("Failed to parse mark booking messages read response")
}

pub async fn get_booking_document_by_number(
    booking_id: Uuid,
    document_number: &str,
    user_jwt: Option<&str>,
) -> Result<TaxDocumentResponse> {
    let api_url = booking_api_url();
    let url = format!(
        "{}/api/v1/bookings/{}/documents/{}",
        api_url, booking_id, document_number
    );
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
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch tax document ({}): {}", status, err_text);
    }
    res.json::<TaxDocumentResponse>()
        .await
        .context("Failed to parse tax document response")
}

pub async fn list_booking_documents(
    booking_id: Uuid,
    user_jwt: Option<&str>,
) -> Result<Vec<TaxDocumentResponse>> {
    let api_url = booking_api_url();
    let url = format!("{}/api/v1/bookings/{}/documents", api_url, booking_id);
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
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to list tax documents ({}): {}", status, err_text);
    }
    res.json::<Vec<TaxDocumentResponse>>()
        .await
        .context("Failed to parse tax documents response")
}
