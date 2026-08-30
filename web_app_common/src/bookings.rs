use common::models::{BookingResponse, NewBookingRequest, UpdatedBookingRequest};
use leptos::prelude::*;
use uuid::Uuid;

#[server]
pub async fn create_booking_api(req: NewBookingRequest) -> Result<BookingResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::create_booking(&req)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = req;
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn get_booking_by_id_api(
    id: Uuid,
    currency: Option<String>,
) -> Result<BookingResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::get_booking_by_id(id, currency.as_deref())
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
pub async fn update_booking_api(
    id: Uuid,
    req: UpdatedBookingRequest,
) -> Result<BookingResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::update_booking(id, &req)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (id, req);
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn delete_booking_api(id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::delete_booking(id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = id;
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn transfer_booking_api(
    booking_id: Uuid,
    new_guest_id: Uuid,
) -> Result<BookingResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::transfer_booking(booking_id, new_guest_id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (booking_id, new_guest_id);
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn get_user_bookings_api(user_id: Uuid) -> Result<Vec<BookingResponse>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::get_user_bookings(user_id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = user_id;
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn get_listing_bookings_api(
    listing_id: Uuid,
) -> Result<Vec<BookingResponse>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        common::app_client::get_listing_bookings(listing_id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = listing_id;
        Err(ServerFnError::new("SSR required"))
    }
}
