use crate::components::protected::RequireAuth;
use leptos::ev::SubmitEvent;
use leptos::form::ActionForm;
use leptos::prelude::*;
use leptos::task::spawn_local;
#[allow(unused_imports)]
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use web_app_common::listings::ListingSearchServer;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateListingParams {
    pub name: String,
    pub user_id: String,
    pub description: Option<String>,
    pub listing_structure: String,
    pub country: String,
    pub base_currency: String,
    pub price_per_night: Option<f64>,
    pub weekly_discount_percentage: Option<f64>,
    pub monthly_discount_percentage: Option<f64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub max_guests: Option<i32>,
    pub bedrooms: Option<i32>,
    pub beds: Option<i32>,
    pub full_bathrooms: Option<i32>,
    pub half_bathrooms: Option<i32>,
    pub square_meters: Option<i32>,
    pub listing_details: Option<String>,
    pub minimum_stay: Option<i32>,
    pub days_between_bookings: Option<i32>,
}

#[cfg(feature = "ssr")]
async fn get_session_context() -> Result<crate::auth::AdminSessionUser, ServerFnError> {
    let session = leptos_actix::extract::<actix_session::Session>()
        .await
        .map_err(|_| ServerFnError::new("Session not found"))?;
    let user_id = session
        .get::<String>("user_id")
        .unwrap_or(None)
        .ok_or_else(|| ServerFnError::new("Unauthorized: Not logged in"))?;
    let user_name = session
        .get::<String>("user_name")
        .unwrap_or_default()
        .unwrap_or_default();
    let user_email = session
        .get::<String>("user_email")
        .unwrap_or_default()
        .unwrap_or_default();
    let is_admin = session
        .get::<bool>("is_admin")
        .unwrap_or(None)
        .unwrap_or(false);
    Ok(crate::auth::AdminSessionUser {
        id: user_id,
        name: user_name,
        email: user_email,
        is_admin,
    })
}

#[cfg(feature = "ssr")]
async fn ensure_listing_owner_or_admin(
    listing_id: &str,
) -> Result<common::models::ListingDetails, ServerFnError> {
    let user = get_session_context().await?;
    let listing_details =
        web_app_common::listings::get_listing_by_id_server(listing_id.to_string(), None).await?;
    if !user.is_admin && listing_details.listing.user_id.to_string() != user.id {
        return Err(ServerFnError::new(
            "Unauthorized: You can only manage your own listings",
        ));
    }
    Ok(listing_details)
}

#[server]
pub async fn create_listing_server(params: CreateListingParams) -> Result<String, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use uuid::Uuid;
        let session_user = get_session_context().await?;
        let target_user_id_str = if session_user.is_admin {
            params.user_id
        } else {
            session_user.id
        };
        let user_id = Uuid::parse_str(&target_user_id_str)
            .map_err(|e| ServerFnError::new(format!("Invalid UUID: {}", e)))?;

        let city = if let (Some(lat), Some(lon)) = (params.latitude, params.longitude) {
            common::geocode::reverse_geocode(lat, lon)
                .await
                .unwrap_or(None)
        } else {
            None
        };

        let request = common::models::NewListingRequest {
            name: params.name,
            user_id,
            description: params.description,
            listing_structure: params.listing_structure,
            country: params.country,
            base_currency: params.base_currency,
            price_per_night: params
                .price_per_night
                .and_then(rust_decimal::Decimal::from_f64),
            weekly_discount_percentage: params
                .weekly_discount_percentage
                .and_then(rust_decimal::Decimal::from_f64),
            monthly_discount_percentage: params
                .monthly_discount_percentage
                .and_then(rust_decimal::Decimal::from_f64),
            latitude: params.latitude,
            longitude: params.longitude,
            city,
            max_guests: params.max_guests.unwrap_or(1),
            bedrooms: params.bedrooms.unwrap_or(0),
            beds: params.beds.unwrap_or(0),
            full_bathrooms: params.full_bathrooms.unwrap_or(0),
            half_bathrooms: params.half_bathrooms.unwrap_or(0),
            square_meters: params.square_meters,
            listing_details: params
                .listing_details
                .and_then(|s| serde_json::from_str(&s).ok()),
            minimum_stay: params.minimum_stay.unwrap_or(1),
            days_between_bookings: params.days_between_bookings.unwrap_or(0),
        };

        let api_url = crate::api_client::listing_api_url();
        let res = crate::api_client::get_client()
            .post(&format!("{}/api/v1/listings", api_url), &api_url, &request)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        if res.status().is_success() {
            let listing: common::models::ListingResponse = res
                .json()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
            Ok(listing.id.to_string())
        } else {
            Err(ServerFnError::new(format!(
                "Failed to create listing: {}",
                res.status()
            )))
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = params;
        Err(ServerFnError::new("SSR required"))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpdateListingParams {
    pub name: Option<String>,
    pub description: Option<String>,
    pub listing_structure: Option<String>,
    pub country: Option<String>,
    pub base_currency: Option<String>,
    pub price_per_night: Option<f64>,
    pub weekly_discount_percentage: Option<f64>,
    pub monthly_discount_percentage: Option<f64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub max_guests: Option<i32>,
    pub bedrooms: Option<i32>,
    pub beds: Option<i32>,
    pub full_bathrooms: Option<i32>,
    pub half_bathrooms: Option<i32>,
    pub square_meters: Option<i32>,
    pub listing_details: Option<String>,
    pub minimum_stay: Option<i32>,
    pub days_between_bookings: Option<i32>,
    pub is_active: Option<bool>,
}

#[server]
pub async fn update_listing_server(
    listing_id: String,
    params: UpdateListingParams,
) -> Result<common::models::ListingResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let _ = ensure_listing_owner_or_admin(&listing_id).await?;

        let city = if let (Some(lat), Some(lon)) = (params.latitude, params.longitude) {
            common::geocode::reverse_geocode(lat, lon)
                .await
                .unwrap_or(None)
        } else {
            None
        };

        let request = common::models::UpdatedListingRequest {
            name: params.name,
            description: params.description,
            listing_structure: params.listing_structure,
            country: params.country,
            base_currency: params.base_currency,
            price_per_night: params
                .price_per_night
                .and_then(rust_decimal::Decimal::from_f64),
            weekly_discount_percentage: params
                .weekly_discount_percentage
                .and_then(rust_decimal::Decimal::from_f64),
            monthly_discount_percentage: params
                .monthly_discount_percentage
                .and_then(rust_decimal::Decimal::from_f64),
            latitude: params.latitude,
            longitude: params.longitude,
            city,
            max_guests: params.max_guests,
            bedrooms: params.bedrooms,
            beds: params.beds,
            full_bathrooms: params.full_bathrooms,
            half_bathrooms: params.half_bathrooms,
            square_meters: params.square_meters,
            listing_details: params
                .listing_details
                .and_then(|s| serde_json::from_str(&s).ok()),
            minimum_stay: params.minimum_stay,
            days_between_bookings: params.days_between_bookings,
            is_active: params.is_active,
        };

        let api_url = crate::api_client::listing_api_url();
        let res = crate::api_client::get_client()
            .patch(
                &format!("{}/api/v1/listings/{}", api_url, listing_id),
                &api_url,
                &request,
            )
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        if res.status().is_success() {
            let listing: common::models::ListingResponse = res
                .json()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
            Ok(listing)
        } else {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            Err(ServerFnError::new(format!(
                "Failed to update listing ({}): {}",
                status, body
            )))
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (listing_id, params);
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn presign_images_server(
    listing_id: String,
    images: Vec<common::models::PendingImageMetadata>,
) -> Result<Vec<common::models::ImagePresignResponse>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let _ = ensure_listing_owner_or_admin(&listing_id).await?;

        let api_url = crate::api_client::listing_api_url();
        let request = common::models::ImagePresignRequest { images };

        let res = crate::api_client::get_client()
            .post(
                &format!("{}/api/v1/listings/{}/images/presign", api_url, listing_id),
                &api_url,
                &request,
            )
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        if res.status().is_success() {
            let presign_res: Vec<common::models::ImagePresignResponse> = res
                .json()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
            Ok(presign_res)
        } else {
            Err(ServerFnError::new(format!(
                "Failed to presign images: {}",
                res.status()
            )))
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (listing_id, images);
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn get_price_overrides_server(
    listing_id: String,
) -> Result<Vec<common::models::PriceOverride>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let _ = ensure_listing_owner_or_admin(&listing_id).await?;

        let api_url = crate::api_client::listing_api_url();
        let res = crate::api_client::get_client()
            .get(
                &format!("{}/api/v1/listings/{}/price-overrides", api_url, listing_id),
                &api_url,
            )
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        if res.status().is_success() {
            let overrides: Vec<common::models::PriceOverride> = res
                .json()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
            Ok(overrides)
        } else {
            Err(ServerFnError::new(format!(
                "Failed to fetch price overrides: {}",
                res.status()
            )))
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = listing_id;
        Ok(Vec::new())
    }
}

#[server]
pub async fn create_price_override_server(
    listing_id: String,
    start_date: String,
    end_date: String,
    nightly_rate: String,
    min_nights: i32,
) -> Result<common::models::PriceOverride, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let _ = ensure_listing_owner_or_admin(&listing_id).await?;

        use chrono::NaiveDate;
        use rust_decimal::Decimal;
        use std::str::FromStr;

        let api_url = crate::api_client::listing_api_url();
        let start = NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
            .map_err(|_| ServerFnError::new("Invalid start date format (YYYY-MM-DD)"))?;
        let end = NaiveDate::parse_from_str(&end_date, "%Y-%m-%d")
            .map_err(|_| ServerFnError::new("Invalid end date format (YYYY-MM-DD)"))?;
        let rate = Decimal::from_str(&nightly_rate)
            .map_err(|_| ServerFnError::new("Invalid nightly rate decimal"))?;

        if end <= start {
            return Err(ServerFnError::new(
                "End date must be strictly after start date",
            ));
        }
        if rate <= Decimal::ZERO {
            return Err(ServerFnError::new("Nightly rate must be greater than zero"));
        }
        if min_nights < 1 {
            return Err(ServerFnError::new("Minimum nights must be at least 1"));
        }

        let request = common::models::CreatePriceOverrideRequest {
            start_date: start,
            end_date: end,
            nightly_rate: rate,
            min_nights,
        };

        let res = crate::api_client::get_client()
            .post(
                &format!("{}/api/v1/listings/{}/price-overrides", api_url, listing_id),
                &api_url,
                &request,
            )
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        if res.status().is_success() {
            let created: common::models::PriceOverride = res
                .json()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
            Ok(created)
        } else {
            Err(ServerFnError::new(format!(
                "Failed to create price override: {}",
                res.status()
            )))
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (listing_id, start_date, end_date, nightly_rate, min_nights);
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn delete_price_override_server(
    listing_id: String,
    override_id: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let _ = ensure_listing_owner_or_admin(&listing_id).await?;

        let api_url = crate::api_client::listing_api_url();
        let res = crate::api_client::get_client()
            .delete(
                &format!(
                    "{}/api/v1/listings/{}/price-overrides/{}",
                    api_url, listing_id, override_id
                ),
                &api_url,
            )
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        if res.status().is_success() {
            Ok(())
        } else {
            Err(ServerFnError::new(format!(
                "Failed to delete price override: {}",
                res.status()
            )))
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (listing_id, override_id);
        Err(ServerFnError::new("SSR required"))
    }
}

#[server]
pub async fn get_listing_bookings_server(
    listing_id: String,
) -> Result<Vec<common::models::BookingResponse>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let _ = ensure_listing_owner_or_admin(&listing_id).await?;
        let lid = uuid::Uuid::parse_str(&listing_id)
            .map_err(|e| ServerFnError::new(format!("Invalid UUID: {}", e)))?;
        web_app_common::bookings::get_listing_bookings_api(lid).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = listing_id;
        Ok(Vec::new())
    }
}

#[server]
pub async fn update_listing_booking_status_server(
    listing_id: String,
    booking_id: String,
    status: String,
) -> Result<common::models::BookingResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let _ = ensure_listing_owner_or_admin(&listing_id).await?;
        let bid = uuid::Uuid::parse_str(&booking_id)
            .map_err(|e| ServerFnError::new(format!("Invalid UUID: {}", e)))?;
        let req = common::models::UpdatedBookingRequest {
            status: Some(status),
            metadata: None,
        };
        web_app_common::bookings::update_booking_api(bid, req).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (listing_id, booking_id, status);
        Err(ServerFnError::new("SSR required"))
    }
}

#[component]
#[allow(non_snake_case)]
pub fn ListingsPage() -> impl IntoView {
    let session_user_resource = Resource::new(
        || (),
        |_| async move { crate::auth::get_current_session_user().await },
    );

    let listing_search = ServerAction::<ListingSearchServer>::new();
    let create_listing = ServerAction::<CreateListingServer>::new();
    let (name, set_name) = signal(None::<String>);
    let (owner_email, set_owner_email) = signal(None::<String>);
    let (max_price, set_max_price) = signal(Some(0.0));
    let (selected_structures, set_selected_structures) = signal(HashSet::<String>::new());

    let (owner_email_input, set_owner_email_input) = signal(String::new());
    let (owner_id_validated, set_owner_id_validated) = signal(None::<String>);
    let (owner_id_error, set_owner_id_error) = signal(false);

    let (uploading_images, set_uploading_images) = signal(false);

    let (next_detail_id, set_next_detail_id) = signal(1usize);

    let (listing_details, set_listing_details) =
        signal(vec![(0usize, String::new(), String::new())]);

    // Signals for Add/Edit Listing form
    let (editing_listing_id, set_editing_listing_id) = signal(None::<String>);
    let (update_loading, set_update_loading) = signal(false);
    let (update_success, set_update_success) = signal(None::<String>);
    let (update_error, set_update_error) = signal(None::<String>);
    let (new_is_active, set_new_is_active) = signal(true);

    let (new_name, set_new_name) = signal(String::new());
    let (new_description, set_new_description) = signal(String::new());
    let (new_listing_structure, set_new_listing_structure) = signal(String::new());
    let (new_country, set_new_country) = signal(String::new());
    let (new_base_currency, set_new_base_currency) = signal("USD".to_string());
    let (new_price_per_night, set_new_price_per_night) = signal(None::<f64>);
    let (new_weekly_discount, set_new_weekly_discount) = signal(None::<f64>);
    let (new_monthly_discount, set_new_monthly_discount) = signal(None::<f64>);
    let (new_latitude, set_new_latitude) = signal(None::<f64>);
    let (new_longitude, set_new_longitude) = signal(None::<f64>);
    let (new_max_guests, set_new_max_guests) = signal(Some(1));
    let (new_bedrooms, set_new_bedrooms) = signal(Some(0));
    let (new_beds, set_new_beds) = signal(Some(0));
    let (new_full_bathrooms, set_new_full_bathrooms) = signal(Some(0));
    let (new_half_bathrooms, set_new_half_bathrooms) = signal(Some(0));
    let (new_square_meters, set_new_square_meters) = signal(None::<i32>);
    let (new_minimum_stay, set_new_minimum_stay) = signal(Some(1));
    let (new_days_between_bookings, set_new_days_between_bookings) = signal(Some(0));

    Effect::new(move |_| {
        if let Some(Ok(Some(user))) = session_user_resource.get() {
            if !user.is_admin {
                set_owner_email.set(Some(user.email.clone()));
                set_owner_email_input.set(user.email.clone());
                set_owner_id_validated.set(Some(user.id.clone()));
                set_owner_id_error.set(false);
                listing_search.dispatch(ListingSearchServer {
                    name: None,
                    owner_email: Some(user.email.clone()),
                    listing_structure: None,
                    max_price: Some(0.0),
                    currency: None,
                });
            } else {
                listing_search.dispatch(ListingSearchServer {
                    name: None,
                    owner_email: None,
                    listing_structure: None,
                    max_price: Some(0.0),
                    currency: None,
                });
            }
        }
    });

    let add_detail = move |_| {
        let id = next_detail_id.get();
        set_next_detail_id.set(id + 1);
        set_listing_details.update(|d| d.push((id, String::new(), String::new())));
    };

    let remove_detail = move |id_to_remove: usize| {
        set_listing_details.update(|d| {
            d.retain(|(id, _, _)| *id != id_to_remove);
            if d.is_empty() {
                let id = next_detail_id.get();
                set_next_detail_id.set(id + 1);
                d.push((id, String::new(), String::new()));
            }
        });
    };

    let update_detail_key = move |id_to_update: usize, key: String| {
        set_listing_details.update(|d| {
            if let Some(pair) = d.iter_mut().find(|(id, _, _)| *id == id_to_update) {
                pair.1 = key;
            }
        });
    };

    let update_detail_value = move |id_to_update: usize, value: String| {
        set_listing_details.update(|d| {
            if let Some(pair) = d.iter_mut().find(|(id, _, _)| *id == id_to_update) {
                pair.2 = value;
            }
        });
    };

    let timeout_handle = StoredValue::new(None::<TimeoutHandle>);

    // 1. Create listing
    // 2. Get number of files being uploaded
    // 3. For each file - add metadata to vec
    // 4. Call presign fn
    // 5. Get back urls
    // 6. For each response we get back create and make a request to url
    Effect::new(move |_| {
        if let Some(Ok(listing_id)) = create_listing.value().get() {
            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            if let Some(document) = window.document() {
                if let Some(element) = document.get_element_by_id("file-upload") {
                    use wasm_bindgen::JsCast;
                    if let Ok(input) = element.dyn_into::<web_sys::HtmlInputElement>() {
                        // Get loaded file(s) info
                        if let Some(files) = input.files() {
                            let count = files.length();
                            if count > 0 {
                                set_uploading_images.set(true);
                                let mut metadata = Vec::new();
                                // Store a mapping of client_file_id -> actual file index
                                let mut local_file_map = std::collections::HashMap::new();

                                // Get and store metadata
                                for i in 0..count {
                                    if let Some(file) = files.item(i) {
                                        let client_file_id = uuid::Uuid::new_v4().to_string();
                                        local_file_map.insert(client_file_id.clone(), i);

                                        metadata.push(common::models::PendingImageMetadata {
                                            client_file_id,
                                            content_type: file.type_(),
                                            size_bytes: file.size() as u64,
                                            display_order: i as i32,
                                        });
                                    }
                                }

                                // Get presigned URL's from backend
                                spawn_local(async move {
                                    match presign_images_server(listing_id.clone(), metadata).await
                                    {
                                        Ok(responses) => {
                                            let mut upload_futures = Vec::new();
                                            for res in responses {
                                                if let Some(&file_idx) =
                                                    local_file_map.get(&res.client_file_id)
                                                {
                                                    if let Some(file) = files.item(file_idx) {
                                                        let url = &res.upload_url;
                                                        let opts = web_sys::RequestInit::new();
                                                        opts.set_method("PUT");
                                                        let js_val: wasm_bindgen::JsValue =
                                                            file.into();
                                                        opts.set_body(&js_val);
                                                        // Upload file to GCS
                                                        if let Ok(request) =
                                                            web_sys::Request::new_with_str_and_init(
                                                                url, &opts,
                                                            )
                                                        {
                                                            let fut = wasm_bindgen_futures::JsFuture::from(
                                                                window.fetch_with_request(&request),
                                                            );
                                                            upload_futures.push(fut);
                                                        }
                                                    }
                                                }
                                            }
                                            futures::future::join_all(upload_futures).await;
                                        }
                                        Err(e) => {
                                            leptos::logging::error!(
                                                "Failed to get presigned URLs: {:?}",
                                                e
                                            );
                                        }
                                    }
                                    set_uploading_images.set(false);
                                });
                            }
                        }
                    }
                }
            }
        }
    });

    let on_email_input = move |ev| {
        let val = event_target_value(&ev).trim().to_string();
        set_owner_email_input.set(val.clone());
        set_owner_id_validated.set(None);

        timeout_handle.update_value(|h: &mut Option<TimeoutHandle>| {
            if let Some(handle) = h.take() {
                handle.clear();
            }
        });

        if val.is_empty() {
            set_owner_id_error.set(false);
            return;
        }

        // Validate email format using validator
        use validator::ValidateEmail;
        if !val.validate_email() {
            set_owner_id_error.set(true);
            return;
        }

        set_owner_id_error.set(false);

        let handle = set_timeout_with_handle(
            move || {
                spawn_local(async move {
                    match crate::components::user::get_user_server(val).await {
                        Ok(user) => {
                            set_owner_id_validated.set(Some(user.id.to_string()));
                            set_owner_id_error.set(false);
                        }
                        Err(_) => {
                            set_owner_id_validated.set(None);
                            set_owner_id_error.set(true);
                        }
                    }
                });
            },
            std::time::Duration::from_millis(500),
        )
        .ok();

        timeout_handle.set_value(handle);
    };

    let listings = Memo::new(move |_| {
        listing_search
            .value()
            .get()
            .unwrap_or_else(|| Ok(vec![]))
            .unwrap_or_default()
    });

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let structures = selected_structures.get();
        let structure_vec: Vec<String> = structures.into_iter().collect();

        let structure_arg = if structure_vec.is_empty() {
            None
        } else {
            Some(structure_vec)
        };

        let effective_owner = if let Some(Ok(Some(user))) = session_user_resource.get() {
            if !user.is_admin {
                Some(user.email)
            } else {
                owner_email.get()
            }
        } else {
            owner_email.get()
        };

        listing_search.dispatch(ListingSearchServer {
            name: name.get(),
            owner_email: effective_owner,
            listing_structure: structure_arg,
            max_price: max_price.get(),
            currency: None,
        });
    };

    let toggle_structure = move |structure: String| {
        set_selected_structures.update(|set| {
            if set.contains(&structure) {
                set.remove(&structure);
            } else {
                set.insert(structure);
            }
        });
    };

    let reset_form = move || {
        set_editing_listing_id.set(None);
        set_update_success.set(None);
        set_update_error.set(None);
        set_new_name.set(String::new());
        set_new_description.set(String::new());
        set_new_listing_structure.set(String::new());
        set_new_country.set(String::new());
        set_new_base_currency.set("USD".to_string());
        set_new_price_per_night.set(None);
        set_new_weekly_discount.set(None);
        set_new_monthly_discount.set(None);
        set_new_latitude.set(None);
        set_new_longitude.set(None);
        set_new_max_guests.set(Some(1));
        set_new_bedrooms.set(Some(0));
        set_new_beds.set(Some(0));
        set_new_full_bathrooms.set(Some(0));
        set_new_half_bathrooms.set(Some(0));
        set_new_square_meters.set(None);
        set_new_minimum_stay.set(Some(1));
        set_new_days_between_bookings.set(Some(0));
        set_new_is_active.set(true);
        set_listing_details.set(vec![(0, String::new(), String::new())]);
        set_next_detail_id.set(1);

        if let Some(Ok(Some(user))) = session_user_resource.get() {
            if !user.is_admin {
                set_owner_email_input.set(user.email.clone());
                set_owner_id_validated.set(Some(user.id.clone()));
                set_owner_id_error.set(false);
            } else {
                set_owner_email_input.set(String::new());
                set_owner_id_validated.set(None);
                set_owner_id_error.set(false);
            }
        } else {
            set_owner_email_input.set(String::new());
            set_owner_id_validated.set(None);
            set_owner_id_error.set(false);
        }
    };

    let populate_fields = move |existing: &common::models::ListingResponse| {
        set_new_name.set(existing.name.clone());
        set_new_description.set(existing.description.clone().unwrap_or_default());
        set_new_listing_structure.set(existing.listing_structure.clone());
        set_new_country.set(existing.country.clone());
        set_new_base_currency.set(existing.base_currency.clone());
        set_new_price_per_night.set(existing.price_per_night.and_then(|p| p.to_f64()));
        set_new_weekly_discount.set(existing.weekly_discount_percentage.and_then(|p| p.to_f64()));
        set_new_monthly_discount.set(
            existing
                .monthly_discount_percentage
                .and_then(|p| p.to_f64()),
        );
        set_new_latitude.set(existing.latitude);
        set_new_longitude.set(existing.longitude);
        set_new_max_guests.set(Some(existing.max_guests));
        set_new_bedrooms.set(Some(existing.bedrooms));
        set_new_beds.set(Some(existing.beds));
        set_new_full_bathrooms.set(Some(existing.full_bathrooms));
        set_new_half_bathrooms.set(Some(existing.half_bathrooms));
        set_new_square_meters.set(existing.square_meters);
        set_new_minimum_stay.set(Some(existing.minimum_stay));
        set_new_days_between_bookings.set(Some(existing.days_between_bookings));
        set_new_is_active.set(existing.is_active);

        // Populate details
        if let Some(details_json) = &existing.listing_details {
            if let Ok(details_map) = serde_json::from_value::<
                std::collections::HashMap<String, String>,
            >(details_json.clone())
            {
                let mut new_details = Vec::new();
                let mut max_id = 0;
                for (k, v) in details_map {
                    new_details.push((max_id, k, v));
                    max_id += 1;
                }
                set_next_detail_id.set(max_id);
                if new_details.is_empty() {
                    new_details.push((0, String::new(), String::new()));
                    set_next_detail_id.set(1);
                }
                set_listing_details.set(new_details);
            }
        } else {
            set_listing_details.set(vec![(0, String::new(), String::new())]);
            set_next_detail_id.set(1);
        }

        // Set owner ID and fetch actual owner email
        let owner_id_str = existing.user_id.to_string();
        set_owner_id_validated.set(Some(owner_id_str.clone()));
        set_owner_id_error.set(false);
        spawn_local(async move {
            match crate::components::user::get_user_server(owner_id_str).await {
                Ok(user) => {
                    set_owner_email_input.set(user.email);
                }
                Err(e) => {
                    leptos::logging::error!("Failed to fetch owner email by id: {:?}", e);
                }
            }
        });
    };

    let populate_from_existing = move |existing: common::models::ListingResponse| {
        set_editing_listing_id.set(None);
        set_update_success.set(None);
        set_update_error.set(None);
        populate_fields(&existing);
    };

    let edit_listing = move |existing: common::models::ListingResponse| {
        set_editing_listing_id.set(Some(existing.id.to_string()));
        set_update_success.set(None);
        set_update_error.set(None);
        populate_fields(&existing);
    };

    let on_form_submit = move |ev: SubmitEvent| {
        if let Some(lid) = editing_listing_id.get() {
            ev.prevent_default();
            set_update_loading.set(true);
            set_update_success.set(None);
            set_update_error.set(None);

            let map: std::collections::HashMap<_, _> = listing_details
                .get()
                .into_iter()
                .filter(|(_, k, _)| !k.is_empty())
                .map(|(_, k, v)| (k, v))
                .collect();
            let details_json = if map.is_empty() {
                None
            } else {
                serde_json::to_string(&map).ok()
            };

            let params = UpdateListingParams {
                name: if new_name.get().is_empty() {
                    None
                } else {
                    Some(new_name.get())
                },
                description: if new_description.get().is_empty() {
                    None
                } else {
                    Some(new_description.get())
                },
                listing_structure: if new_listing_structure.get().is_empty() {
                    None
                } else {
                    Some(new_listing_structure.get())
                },
                country: if new_country.get().is_empty() {
                    None
                } else {
                    Some(new_country.get())
                },
                base_currency: if new_base_currency.get().is_empty() {
                    None
                } else {
                    Some(new_base_currency.get())
                },
                price_per_night: new_price_per_night.get(),
                weekly_discount_percentage: new_weekly_discount.get(),
                monthly_discount_percentage: new_monthly_discount.get(),
                latitude: new_latitude.get(),
                longitude: new_longitude.get(),
                max_guests: new_max_guests.get(),
                bedrooms: new_bedrooms.get(),
                beds: new_beds.get(),
                full_bathrooms: new_full_bathrooms.get(),
                half_bathrooms: new_half_bathrooms.get(),
                square_meters: new_square_meters.get(),
                listing_details: details_json,
                minimum_stay: new_minimum_stay.get(),
                days_between_bookings: new_days_between_bookings.get(),
                is_active: Some(new_is_active.get()),
            };

            spawn_local(async move {
                match update_listing_server(lid, params).await {
                    Ok(updated) => {
                        set_update_success.set(Some(format!(
                            "Listing '{}' updated successfully!",
                            updated.name
                        )));
                        set_update_loading.set(false);
                        let structures = selected_structures.get();
                        let structure_vec: Vec<String> = structures.into_iter().collect();
                        let structure_arg = if structure_vec.is_empty() {
                            None
                        } else {
                            Some(structure_vec)
                        };
                        listing_search.dispatch(ListingSearchServer {
                            name: name.get(),
                            owner_email: owner_email.get(),
                            listing_structure: structure_arg,
                            max_price: max_price.get(),
                            currency: None,
                        });
                    }
                    Err(e) => {
                        set_update_error.set(Some(e.to_string()));
                        set_update_loading.set(false);
                    }
                }
            });
        }
    };

    view! {
        <RequireAuth>
            <h1>"Listings Page"</h1>
            <div class="flex w-full flex-col lg:flex-row">
                <div class="card bg-base-300 rounded-box grid h-32 grow place-items-center">
                    <h2>Search Listings</h2>
                    <div class="flex w-full flex-col lg:flex-row">
                        <div class="card bg-base-300 rounded-box grid grow p-4">
                            <div class="flex flex-col mb-4">
                                <form on:submit=on_submit class="form-control w-full space-y-4">
                                    <div class="flex flex-wrap gap-4 items-end">
                                        {move || {
                                            let is_admin = session_user_resource.get()
                                                .and_then(|r| r.ok())
                                                .flatten()
                                                .map(|u| u.is_admin)
                                                .unwrap_or(false);
                                            if is_admin {
                                                view! {
                                                    <div class="form-control w-full max-w-xs">
                                                        <label class="label">
                                                            <span class="label-text">Owner Email</span>
                                                        </label>
                                                        <label class="input input-bordered flex items-center gap-2">
                                                            <svg class="h-[1em] opacity-50" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                                                                <g stroke-linejoin="round" stroke-linecap="round" stroke-width="2.5" fill="none" stroke="currentColor">
                                                                    <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"></path>
                                                                    <circle cx="12" cy="7" r="4"></circle>
                                                                </g>
                                                            </svg>
                                                            <input
                                                                type="email"
                                                                class="grow"
                                                                placeholder="username@domain.com"
                                                                on:input=move |ev| set_owner_email.set(Some(event_target_value(&ev)))
                                                                prop:value=move || owner_email.get().unwrap_or_default()
                                                            />
                                                        </label>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                ().into_any()
                                            }
                                        }}

                                        <div class="form-control w-full max-w-xs">
                                            <label class="label">
                                                <span class="label-text">Listing Name</span>
                                            </label>
                                            <label class="input input-bordered flex items-center gap-2">
                                                <svg class="h-[1em] opacity-50" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                                                    <g stroke-linejoin="round" stroke-linecap="round" stroke-width="2.5" fill="none" stroke="currentColor">
                                                        <circle cx="11" cy="11" r="8"></circle>
                                                        <path d="m21 21-4.3-4.3"></path>
                                                    </g>
                                                </svg>
                                                <input
                                                    type="search"
                                                    class="grow"
                                                    placeholder="Listing name"
                                                    on:input=move |ev| set_name.set(Some(event_target_value(&ev)))
                                                    prop:value=move || name.get().unwrap_or_default()
                                                />
                                            </label>
                                        </div>

                                        <div class="form-control">
                                            <label class="label cursor-pointer">
                                                <input type="submit" value="Search" class="btn btn-primary" />
                                            </label>
                                        </div>
                                    </div>
                                </form>

                                <details class="collapse bg-base-100 border border-base-300 collapse-plus">
                                    <summary class="collapse-title font-semibold">Additional filters</summary>
                                    <div class="collapse-content text-sm space-y-4">
                                        <div class="form-control w-full max-w-xs">
                                            <fieldset class="fieldset bg-base-100 border-base-300 rounded-box w-64 border p-4">
                                                <legend class="fieldset-legend">Property Type</legend>
                                                <ul>
                                                    <li>
                                                        <label class="label">
                                                            <input
                                                                type="checkbox"
                                                                class="checkbox"
                                                                on:change=move |_| toggle_structure("Apartment".to_string())
                                                                prop:checked=move || selected_structures.get().contains("Apartment")
                                                        />
                                                        Apartment
                                                        </label>
                                                    </li>
                                                    <li>
                                                        <label class="label">
                                                            <input
                                                                type="checkbox"
                                                                class="checkbox"
                                                                on:change=move |_| toggle_structure("Townhouse".to_string())
                                                                prop:checked=move || selected_structures.get().contains("Townhouse")
                                                        />
                                                        Townhouse
                                                        </label>
                                                    </li>
                                                    <li>
                                                        <label class="label">
                                                            <input
                                                                type="checkbox"
                                                                class="checkbox"
                                                                on:change=move |_| toggle_structure("Studio".to_string())
                                                                prop:checked=move || selected_structures.get().contains("Studio")
                                                        />
                                                        Studio
                                                        </label>
                                                    </li>
                                                    <li>
                                                        <label class="label">
                                                            <input
                                                                type="checkbox"
                                                                class="checkbox"
                                                                on:change=move |_| toggle_structure("House".to_string())
                                                                prop:checked=move || selected_structures.get().contains("House")
                                                        />
                                                        House
                                                        </label>
                                                    </li>
                                                    <li>
                                                        <label class="label">
                                                            <input
                                                                type="checkbox"
                                                                class="checkbox"
                                                                on:change=move |_| toggle_structure("Villa".to_string())
                                                                prop:checked=move || selected_structures.get().contains("Villa")
                                                        />
                                                        Villa
                                                        </label>
                                                    </li>
                                                </ul>
                                            </fieldset>
                                        </div>

                                        <div class="form-control w-full max-w-xs">
                                            <label class="label">
                                                <span class="label-text">Max Price: <span id="price-val">{move || max_price.get().unwrap_or(0.0)}</span></span>
                                            </label>
                                            <input
                                                type="range"
                                                min="0"
                                                max="1000"
                                                step="10"
                                                class="range range-primary"
                                                on:input=move |ev| {
                                                    if let Ok(val) = event_target_value(&ev).parse::<f64>() {
                                                        set_max_price.set(Some(val));
                                                    }
                                                }
                                                prop:value=move || max_price.get().unwrap_or(0.0)
                                            />
                                        </div>
                                    </div>
                                </details>
                            </div>

                            <div class="space-y-4">
                                <For
                                    each=move || listings.get()
                                    key=|listing| listing.id
                                    children=move |listing| {
                                        let listing_for_edit = listing.clone();
                                        let listing_for_populate = listing.clone();
                                        let (show_overrides, set_show_overrides) = signal(false);
                                        let listing_id_str = listing.id.to_string();
                                        let listing_name_str = listing.name.clone();

                                        view! {
                                            <div class="card bg-base-100 shadow-sm flex flex-col p-4 mb-4">
                                                <div class="flex flex-row">
                                                    <figure class="w-48 h-48 flex-none">
                                                        <img
                                                            class="h-full w-full object-cover"
                                                            src={listing.primary_image_url.clone().unwrap_or_else(|| "https://img.daisyui.com/images/stock/photo-1635805737707-575885ab0820.webp".to_string())}
                                                            alt="Listing Image" />
                                                    </figure>
                                                    <div class="card-body">
                                                        <h2 class="card-title">{listing.name.clone()}</h2>
                                                        <p class="text-sm text-gray-500">
                                                            "Owner: " {listing.owner_name.clone().unwrap_or_else(|| "Unknown".to_string())}
                                                        </p>
                                                        <p class="text-sm">{listing.description.clone().unwrap_or_default()}</p>
                                                        <div class="card-actions justify-end">
                                                            <span class="badge badge-outline">{listing.listing_structure.clone()}</span>
                                                            <span class="badge badge-ghost">
                                                                {listing.price_per_night.map(|p| format!("${}", p)).unwrap_or_default()}
                                                            </span>
                                                            <button
                                                                class="btn btn-accent btn-sm"
                                                                on:click=move |_| set_show_overrides.update(|v| *v = !*v)
                                                            >
                                                                {move || if show_overrides.get() { "Hide Manage Section" } else { "Manage Listing (Bookings, Rates & Reviews)" }}
                                                            </button>
                                                            <button
                                                                class="btn btn-primary btn-sm"
                                                                on:click=move |_| edit_listing(listing_for_edit.clone())
                                                            >
                                                                "Edit Listing"
                                                            </button>
                                                            <button
                                                                class="btn btn-secondary btn-sm"
                                                                on:click=move |_| populate_from_existing(listing_for_populate.clone())
                                                            >
                                                                "Populate"
                                                            </button>
                                                        </div>
                                                    </div>
                                                </div>
                                                {move || {
                                                    if show_overrides.get() {
                                                        view! {
                                                            <div class="flex flex-col gap-4">
                                                                <ListingBookingsAdminSection
                                                                    listing_id=listing_id_str.clone()
                                                                    listing_name=listing_name_str.clone()
                                                                />
                                                                <PriceOverridesSection
                                                                    listing_id=listing_id_str.clone()
                                                                    listing_name=listing_name_str.clone()
                                                                />
                                                                <ListingReviewsAdminSection
                                                                    listing_id=listing_id_str.clone()
                                                                    listing_name=listing_name_str.clone()
                                                                />
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        ().into_any()
                                                    }
                                                }}
                                            </div>
                                        }
                                    }
                                />
                            </div>
                            {move || {
                                if listing_search.pending().get() {
                                    view! { <span class="loading loading-spinner loading-md">Loading...</span> }.into_any()
                                } else if listings.get().is_empty() && listing_search.input().with(|i| i.is_some()) {
                                    view! {
                                        <div class="alert alert-info">
                                            <span>"No listings found match your criteria."</span>
                                        </div>
                                    }.into_any()
                                } else {
                                    ().into_any()
                                }
                            }}
                        </div>
                    </div>
                </div>
                <div class="divider lg:divider-horizontal">-</div>
                <div class="card bg-base-300 rounded-box grid grow place-items-center p-4">
                    <h2 class="text-xl font-bold">
                        {move || if editing_listing_id.get().is_some() {
                            format!("Edit Listing: {}", new_name.get())
                        } else {
                            "Add New Listing".to_string()
                        }}
                    </h2>
                    <ActionForm action={create_listing} on:submit=on_form_submit attr:class="form-control w-full max-w-xs space-y-4">
                        <div>
                            <label for="listing_name" class="label">
                                <span class="label-text">Listing Name</span>
                            </label>
                            <input
                                type="text"
                                name="params[name]"
                                placeholder="Listing Name"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_name.set(event_target_value(&ev))
                                prop:value=move || new_name.get()
                                required
                            />
                        </div>
                        <div>
                            <label for="owner_email" class="label">
                                <span class="label-text">Owner Email</span>
                            </label>
                            {move || {
                                let is_admin = session_user_resource.get()
                                    .and_then(|r| r.ok())
                                    .flatten()
                                    .map(|u| u.is_admin)
                                    .unwrap_or(false);
                                if is_admin {
                                    view! {
                                        <label
                                            class=move || {
                                                if owner_id_validated.get().is_some() {
                                                    "input input-bordered flex items-center gap-2 w-full max-w-xs input-success"
                                                } else if owner_id_error.get() {
                                                    "input input-bordered flex items-center gap-2 w-full max-w-xs input-error"
                                                } else {
                                                    "input input-bordered flex items-center gap-2 w-full max-w-xs"
                                                }
                                            }
                                        >
                                            <input
                                                type="email"
                                                placeholder="Owner Email (e.g. host@example.com)"
                                                class="grow"
                                                on:input=on_email_input
                                                prop:value=move || owner_email_input.get()
                                            />
                                            {move || {
                                                if owner_id_validated.get().is_some() {
                                                    view! {
                                                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="fill-green-500 size-4">
                                                            <path fill-rule="evenodd" d="M12.416 3.376a.75.75 0 0 1 .208 1.04l-5 7.5a.75.75 0 0 1-1.154.114l-3-3a.75.75 0 0 1 1.06-1.06l2.353 2.353 4.493-6.74a.75.75 0 0 1 1.04-.207Z" clip-rule="evenodd" />
                                                        </svg>
                                                    }.into_any()
                                                } else if owner_id_error.get() {
                                                    view! {
                                                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="fill-red-500 size-4">
                                                            <path d="M5.28 4.22a.75.75 0 0 0-1.06 1.06L6.94 8l-2.72 2.72a.75.75 0 1 0 1.06 1.06L8 9.06l2.72 2.72a.75.75 0 1 0 1.06-1.06L9.06 8l2.72-2.72a.75.75 0 0 0-1.06-1.06L8 6.94 5.28 4.22Z" />
                                                        </svg>
                                                    }.into_any()
                                                } else {
                                                    ().into_any()
                                                }
                                            }}
                                        </label>
                                    }.into_any()
                                } else {
                                    view! {
                                        <label class="input input-bordered flex items-center gap-2 w-full max-w-xs bg-base-200 cursor-not-allowed">
                                            <input
                                                type="email"
                                                class="grow bg-transparent cursor-not-allowed text-base-content/70"
                                                disabled
                                                prop:value=move || owner_email_input.get()
                                            />
                                            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="fill-green-500 size-4">
                                                <path fill-rule="evenodd" d="M12.416 3.376a.75.75 0 0 1 .208 1.04l-5 7.5a.75.75 0 0 1-1.154.114l-3-3a.75.75 0 0 1 1.06-1.06l2.353 2.353 4.493-6.74a.75.75 0 0 1 1.04-.207Z" clip-rule="evenodd" />
                                            </svg>
                                        </label>
                                    }.into_any()
                                }
                            }}
                            <input type="hidden" name="params[user_id]" value=move || owner_id_validated.get().unwrap_or_default() />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Description</span>
                            </label>
                            <textarea
                                name="params[description]"
                                placeholder="Description"
                                class="textarea textarea-bordered h-24 w-full max-w-xs"
                                on:input=move |ev| set_new_description.set(event_target_value(&ev))
                                prop:value=move || new_description.get()
                            ></textarea>
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Structure Type</span>
                            </label>
                            <select
                                name="params[listing_structure]"
                                class="select select-bordered w-full max-w-xs"
                                on:change=move |ev| set_new_listing_structure.set(event_target_value(&ev))
                                prop:value=move || new_listing_structure.get()
                            >
                                <option disabled selected=move || new_listing_structure.get().is_empty()>Select property type</option>
                                <option value="Apartment">Apartment</option>
                                <option value="House">House</option>
                                <option value="Studio">Studio</option>
                                <option value="Townhouse">Townhouse</option>
                                <option value="Villa">Villa</option>
                            </select>
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Country</span>
                            </label>
                            <select
                                name="params[country]"
                                class="select select-bordered w-full max-w-xs"
                                on:change=move |ev| set_new_country.set(event_target_value(&ev))
                                prop:value=move || new_country.get()
                                required
                            >
                                <option disabled selected=move || new_country.get().is_empty() value="">"Select country"</option>
                                {common::reference::SupportedCountry::LIST.iter().map(|c| {
                                    view! { <option value=c.iso2char>{c.name}</option> }
                                }).collect::<Vec<_>>()}
                            </select>
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Base Currency</span>
                            </label>
                            <select
                                name="params[base_currency]"
                                class="select select-bordered w-full max-w-xs"
                                on:change=move |ev| set_new_base_currency.set(event_target_value(&ev))
                                prop:value=move || new_base_currency.get()
                                required
                            >
                                <option disabled selected=move || new_base_currency.get().is_empty() value="">"Select base currency"</option>
                                <option value="USD">"USD - US Dollar"</option>
                                <option value="JMD">"JMD - Jamaican Dollar"</option>
                                <option value="GBP">"GBP - British Pound"</option>
                            </select>
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Price Per Night ($)</span>
                            </label>
                            <input
                                type="number"
                                step="0.50"
                                min="0"
                                name="params[price_per_night]"
                                placeholder="0.00"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_price_per_night.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_price_per_night.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Weekly Discount (%)</span>
                            </label>
                            <input
                                type="number"
                                step="0.1"
                                min="0"
                                max="100"
                                name="params[weekly_discount_percentage]"
                                placeholder="0.0"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_weekly_discount.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_weekly_discount.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Monthly Discount (%)</span>
                            </label>
                            <input
                                type="number"
                                step="0.1"
                                min="0"
                                max="100"
                                name="params[monthly_discount_percentage]"
                                placeholder="0.0"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_monthly_discount.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_monthly_discount.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Latitude</span>
                            </label>
                            <input
                                type="number"
                                step="0.000001"
                                min="-90"
                                max="90"
                                name="params[latitude]"
                                placeholder="0.000000"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_latitude.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_latitude.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Longitude</span>
                            </label>
                            <input
                                type="number"
                                step="0.000001"
                                min="-180"
                                max="180"
                                name="params[longitude]"
                                placeholder="0.000000"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_longitude.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_longitude.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Max Guests</span>
                            </label>
                            <input
                                type="number"
                                min="1"
                                name="params[max_guests]"
                                placeholder="1"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_max_guests.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_max_guests.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Bedrooms</span>
                            </label>
                            <input
                                type="number"
                                min="0"
                                name="params[bedrooms]"
                                placeholder="0"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_bedrooms.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_bedrooms.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Beds</span>
                            </label>
                            <input
                                type="number"
                                min="0"
                                name="params[beds]"
                                placeholder="0"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_beds.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_beds.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Full Bathrooms</span>
                            </label>
                            <input
                                type="number"
                                min="0"
                                name="params[full_bathrooms]"
                                placeholder="0"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_full_bathrooms.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_full_bathrooms.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Half Bathrooms</span>
                            </label>
                            <input
                                type="number"
                                min="0"
                                name="params[half_bathrooms]"
                                placeholder="0"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_half_bathrooms.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_half_bathrooms.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Square Meters</span>
                            </label>
                            <input
                                type="number"
                                min="0"
                                name="params[square_meters]"
                                placeholder="e.g. 100"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_square_meters.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_square_meters.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Minimum Stay (Nights)</span>
                            </label>
                            <input
                                type="number"
                                min="1"
                                name="params[minimum_stay]"
                                placeholder="1"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_minimum_stay.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_minimum_stay.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Days Between Bookings</span>
                            </label>
                            <input
                                type="number"
                                min="0"
                                name="params[days_between_bookings]"
                                placeholder="0"
                                class="input input-bordered w-full max-w-xs"
                                on:input=move |ev| set_new_days_between_bookings.set(event_target_value(&ev).parse().ok())
                                prop:value=move || new_days_between_bookings.get().map(|v| v.to_string()).unwrap_or_default()
                            />
                        </div>
                        <div class="w-full max-w-xs flex flex-col">
                            <label class="label">
                                <span class="label-text">Listing Details</span>
                            </label>

                            <For
                                each=move || listing_details.get()
                                key=|(id, _, _)| *id
                                children=move |(item_id, item_key, item_value)| {
                                    view! {
                                        <div class="flex items-center space-x-2 w-full mt-2">
                                            <input
                                                type="text"
                                                class="input input-bordered w-full"
                                                placeholder="Detail"
                                                list="details-options"
                                                value=item_key
                                                on:input=move |ev| update_detail_key(item_id, event_target_value(&ev))
                                            />
                                            <input
                                                type="text"
                                                class="input input-bordered w-full"
                                                placeholder="Value"
                                                value=item_value
                                                on:input=move |ev| update_detail_value(item_id, event_target_value(&ev))
                                            />
                                            <button
                                                type="button"
                                                class="btn btn-square btn-outline btn-error btn-sm w-12"
                                                on:click=move |_| remove_detail(item_id)
                                            >
                                                "✗"
                                            </button>
                                        </div>
                                    }
                                }
                            />

                            <button
                                type="button"
                                class="btn btn-sm btn-outline mt-4 w-full"
                                on:click=add_detail
                            >
                                "+ Add Detail"
                            </button>

                            <datalist id="details-options">
                                <option value="WiFi"></option>
                                <option value="Parking"></option>
                                <option value="Pool"></option>
                                <option value="Gym"></option>
                                <option value="Air Conditioning"></option>
                                <option value="Heating"></option>
                                <option value="Pet Friendly"></option>
                                <option value="Kitchen"></option>
                                <option value="Workspace"></option>
                                <option value="TV"></option>
                                <option value="Washer"></option>
                                <option value="Dryer"></option>
                                <option value="Hot Tub"></option>
                                <option value="Balcony"></option>
                            </datalist>

                            <input
                                type="hidden"
                                name="params[listing_details]"
                                value=move || {
                                    let map: std::collections::HashMap<_, _> = listing_details.get().into_iter()
                                        .filter(|(_, k, _)| !k.is_empty())
                                        .map(|(_, k, v)| (k, v))
                                        .collect();
                                    if map.is_empty() {
                                        String::new()
                                    } else {
                                        serde_json::to_string(&map).unwrap_or_default()
                                    }
                                }
                            />
                        </div>
                        {move || if editing_listing_id.get().is_some() {
                            view! {
                                <div class="form-control">
                                    <label class="label cursor-pointer justify-between">
                                        <span class="label-text font-semibold">Active Listing</span>
                                        <input
                                            type="checkbox"
                                            class="toggle toggle-primary"
                                            on:change=move |ev| set_new_is_active.set(event_target_checked(&ev))
                                            prop:checked=move || new_is_active.get()
                                        />
                                    </label>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div>
                                    <label class="label">
                                        <span class="label-text">Upload images (max 10)</span>
                                    </label>
                                    <input type="file" id="file-upload" multiple />
                                </div>
                            }.into_any()
                        }}

                        <div class="flex gap-2 items-center pt-2">
                            {move || if editing_listing_id.get().is_some() {
                                view! {
                                    <button
                                        type="submit"
                                        class="btn btn-primary grow"
                                        disabled=move || update_loading.get()
                                    >
                                        {move || if update_loading.get() { "Saving Changes..." } else { "Save Changes" }}
                                    </button>
                                    <button
                                        type="button"
                                        class="btn btn-ghost"
                                        on:click=move |_| reset_form()
                                    >
                                        "Cancel"
                                    </button>
                                }.into_any()
                            } else {
                                view! {
                                    <button
                                        type="submit"
                                        class="btn btn-primary w-full"
                                        disabled=move || create_listing.pending().get() || owner_id_validated.get().is_none() || uploading_images.get()
                                    >
                                        {move || {
                                            if create_listing.pending().get() {
                                                "Creating..."
                                            } else if uploading_images.get() {
                                                "Uploading Images..."
                                            } else {
                                                "Create Listing"
                                            }
                                        }}
                                    </button>
                                }.into_any()
                            }}
                        </div>

                        {move || update_success.get().map(|msg| view! {
                            <div class="alert alert-success mt-4"><span>{msg}</span></div>
                        })}
                        {move || update_error.get().map(|err| view! {
                            <div class="alert alert-error mt-4"><span>{err}</span></div>
                        })}
                        {move || if editing_listing_id.get().is_none() {
                            create_listing.value().get().map(|v| match v {
                                Ok(_) => view! { <div class="alert alert-success mt-4"><span>"Listing created successfully"</span></div> }.into_any(),
                                Err(e) => view! { <div class="alert alert-error mt-4"><span>{e.to_string()}</span></div> }.into_any(),
                            })
                        } else {
                            None
                        }}
                    </ActionForm>
                </div>
            </div>
        </RequireAuth>
    }
}

#[component]
#[allow(non_snake_case)]
pub fn ListingBookingsAdminSection(listing_id: String, listing_name: String) -> impl IntoView {
    let (refresh_trigger, set_refresh_trigger) = signal(0);
    let (action_message, set_action_message) = signal(None::<(bool, String)>);
    let (loading_action, set_loading_action) = signal(false);

    let lid_for_resource = listing_id.clone();
    let bookings_resource = Resource::new(
        move || (lid_for_resource.clone(), refresh_trigger.get()),
        |(lid, _)| async move { get_listing_bookings_server(lid).await },
    );

    let lid_for_actions = listing_id.clone();

    let update_status = move |booking_id: String, new_status: String| {
        let lid = lid_for_actions.clone();
        set_loading_action.set(true);
        set_action_message.set(None);
        spawn_local(async move {
            match update_listing_booking_status_server(lid, booking_id, new_status.clone()).await {
                Ok(_) => {
                    set_action_message.set(Some((
                        true,
                        format!("Booking status updated to '{}' successfully!", new_status),
                    )));
                    set_refresh_trigger.update(|v| *v += 1);
                }
                Err(e) => {
                    set_action_message.set(Some((
                        false,
                        format!("Failed to update booking status: {}", e),
                    )));
                }
            }
            set_loading_action.set(false);
        });
    };

    view! {
        <div class="mt-4 p-4 border border-base-300 rounded-box bg-base-200 space-y-4">
            <div class="flex justify-between items-center">
                <h3 class="font-bold text-md text-primary">"Bookings & Reservations — " {listing_name}</h3>
                {move || if loading_action.get() {
                    view! { <span class="loading loading-spinner loading-xs text-primary"></span> }.into_any()
                } else {
                    ().into_any()
                }}
            </div>

            {move || action_message.get().map(|(is_success, msg)| {
                let alert_class = if is_success { "alert alert-success text-xs p-2" } else { "alert alert-error text-xs p-2" };
                view! {
                    <div class=alert_class>
                        <span>{msg}</span>
                    </div>
                }
            })}

            <Suspense fallback=move || view! { <div class="loading loading-spinner loading-sm"></div> }>
                {move || bookings_resource.get().map(|res| match res {
                    Ok(bookings) => {
                        if bookings.is_empty() {
                            view! {
                                <p class="text-sm text-base-content/70">"No bookings found for this listing."</p>
                            }.into_any()
                        } else {
                            view! {
                                <div class="overflow-x-auto">
                                    <table class="table table-zebra w-full text-xs">
                                        <thead>
                                            <tr>
                                                <th>"Confirmation"</th>
                                                <th>"Dates"</th>
                                                <th>"Guests"</th>
                                                <th>"Total"</th>
                                                <th>"Status"</th>
                                                <th>"Actions"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            <For
                                                each=move || bookings.clone()
                                                key=|b| b.id
                                                children={
                                                    let update_fn = update_status.clone();
                                                    move |booking| {
                                                        let b_id = booking.id.to_string();
                                                        let b_id_confirm = b_id.clone();
                                                        let b_id_cancel = b_id.clone();
                                                        let b_id_complete = b_id.clone();
                                                        let status_lower = booking.status.to_lowercase();
                                                        let status_badge_class = match status_lower.as_str() {
                                                            "confirmed" => "badge badge-success badge-sm",
                                                            "pending" => "badge badge-warning badge-sm",
                                                            "completed" => "badge badge-info badge-sm",
                                                            "cancelled" => "badge badge-error badge-sm",
                                                            _ => "badge badge-ghost badge-sm",
                                                        };

                                                        let update_c = update_fn.clone();
                                                        let update_canc = update_fn.clone();
                                                        let update_comp = update_fn.clone();

                                                        view! {
                                                            <tr>
                                                                <td class="font-mono font-bold text-primary">{booking.confirmation_code.clone()}</td>
                                                                <td>{format!("{} → {}", booking.date_from, booking.date_to)}</td>
                                                                <td>{booking.number_of_persons}</td>
                                                                <td class="font-semibold">{format!("{} {}", booking.currency, booking.total_price)}</td>
                                                                <td><span class=status_badge_class>{booking.status.clone()}</span></td>
                                                                <td>
                                                                    <div class="flex gap-1">
                                                                        {if status_lower == "pending" {
                                                                            view! {
                                                                                <button
                                                                                    class="btn btn-success btn-xs"
                                                                                    on:click=move |_| update_c(b_id_confirm.clone(), "confirmed".to_string())
                                                                                >
                                                                                    "Confirm"
                                                                                </button>
                                                                                <button
                                                                                    class="btn btn-error btn-xs"
                                                                                    on:click=move |_| update_canc(b_id_cancel.clone(), "cancelled".to_string())
                                                                                >
                                                                                    "Cancel"
                                                                                </button>
                                                                            }.into_any()
                                                                        } else if status_lower == "confirmed" {
                                                                            view! {
                                                                                <button
                                                                                    class="btn btn-info btn-xs"
                                                                                    on:click=move |_| update_comp(b_id_complete.clone(), "completed".to_string())
                                                                                >
                                                                                    "Complete"
                                                                                </button>
                                                                                <button
                                                                                    class="btn btn-error btn-xs"
                                                                                    on:click=move |_| update_canc(b_id_cancel.clone(), "cancelled".to_string())
                                                                                >
                                                                                    "Cancel"
                                                                                </button>
                                                                            }.into_any()
                                                                        } else {
                                                                            view! {
                                                                                <span class="text-[10px] opacity-60">"None"</span>
                                                                            }.into_any()
                                                                        }}
                                                                    </div>
                                                                </td>
                                                            </tr>
                                                        }
                                                    }
                                                }
                                            />
                                        </tbody>
                                    </table>
                                </div>
                            }.into_any()
                        }
                    }
                    Err(e) => view! {
                        <div class="alert alert-error text-xs p-2">
                            <span>{e.to_string()}</span>
                        </div>
                    }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

#[component]
#[allow(non_snake_case)]
pub fn PriceOverridesSection(listing_id: String, listing_name: String) -> impl IntoView {
    let (overrides, set_overrides) = signal(Vec::<common::models::PriceOverride>::new());
    let (loading, set_loading) = signal(false);
    let (error_msg, set_error_msg) = signal(None::<String>);

    let (start_date, set_start_date) = signal(String::new());
    let (end_date, set_end_date) = signal(String::new());
    let (nightly_rate, set_nightly_rate) = signal(String::new());
    let (min_nights, set_min_nights) = signal(1i32);

    let lid_for_effect = listing_id.clone();
    Effect::new(move |_| {
        let lid = lid_for_effect.clone();
        spawn_local(async move {
            set_loading.set(true);
            set_error_msg.set(None);
            match get_price_overrides_server(lid).await {
                Ok(data) => set_overrides.set(data),
                Err(e) => set_error_msg.set(Some(e.to_string())),
            }
            set_loading.set(false);
        });
    });

    let lid_for_add = listing_id.clone();
    let on_add_override = move |ev: SubmitEvent| {
        ev.prevent_default();
        let lid = lid_for_add.clone();
        let s_date = start_date.get();
        let e_date = end_date.get();
        let rate = nightly_rate.get();
        let min_n = min_nights.get();

        if s_date.is_empty() || e_date.is_empty() || rate.is_empty() {
            set_error_msg.set(Some("Please fill out all required fields.".to_string()));
            return;
        }

        set_loading.set(true);
        spawn_local(async move {
            match create_price_override_server(lid.clone(), s_date, e_date, rate, min_n).await {
                Ok(_) => {
                    set_start_date.set(String::new());
                    set_end_date.set(String::new());
                    set_nightly_rate.set(String::new());
                    set_min_nights.set(1);
                    if let Ok(data) = get_price_overrides_server(lid).await {
                        set_overrides.set(data);
                    }
                    set_loading.set(false);
                }
                Err(e) => {
                    set_error_msg.set(Some(e.to_string()));
                    set_loading.set(false);
                }
            }
        });
    };

    let lid_for_del = listing_id.clone();

    view! {
        <div class="mt-4 p-4 border border-base-300 rounded-box bg-base-200 space-y-4">
            <div class="flex justify-between items-center">
                <h3 class="font-bold text-md text-primary">"Seasonal Rates & Pricing Overrides — " {listing_name}</h3>
            </div>

            {move || error_msg.get().map(|msg| view! {
                <div class="alert alert-error text-xs p-2">
                    <span>{msg}</span>
                </div>
            })}

            <div class="overflow-x-auto">
                <table class="table table-zebra w-full text-xs">
                    <thead>
                        <tr>
                            <th>"Start Date"</th>
                            <th>"End Date"</th>
                            <th>"Nightly Rate"</th>
                            <th>"Min Stay"</th>
                            <th>"Action"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || overrides.get()
                            key=|ovr| ovr.id
                            children={
                                let lid_base = lid_for_del.clone();
                                move |ovr| {
                                    let ovr_id = ovr.id.to_string();
                                    let lid_for_item = lid_base.clone();
                                    view! {
                                        <tr>
                                            <td>{ovr.start_date.to_string()}</td>
                                            <td>{ovr.end_date.to_string()}</td>
                                            <td class="font-semibold text-primary">"$" {ovr.nightly_rate.to_string()}</td>
                                            <td>{ovr.min_nights} " nights"</td>
                                            <td>
                                                <button
                                                    class="btn btn-error btn-xs"
                                                    on:click=move |_| {
                                                        let lid_c = lid_for_item.clone();
                                                        let ovr_c = ovr_id.clone();
                                                        set_loading.set(true);
                                                        spawn_local(async move {
                                                            match delete_price_override_server(lid_c.clone(), ovr_c).await {
                                                                Ok(_) => {
                                                                    if let Ok(data) = get_price_overrides_server(lid_c).await {
                                                                        set_overrides.set(data);
                                                                    }
                                                                    set_loading.set(false);
                                                                }
                                                                Err(e) => {
                                                                    set_error_msg.set(Some(e.to_string()));
                                                                    set_loading.set(false);
                                                                }
                                                            }
                                                        });
                                                    }
                                                >
                                                    "Delete"
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                }
                            }
                        />
                    </tbody>
                </table>
            </div>


            <form on:submit=on_add_override class="flex flex-wrap gap-2 items-end pt-2 border-t border-base-300">
                <div class="form-control">
                    <label class="label py-1"><span class="label-text text-xs">Start Date</span></label>
                    <input
                        type="date"
                        class="input input-bordered input-xs"
                        on:input=move |ev| set_start_date.set(event_target_value(&ev))
                        prop:value=move || start_date.get()
                        required
                    />
                </div>
                <div class="form-control">
                    <label class="label py-1"><span class="label-text text-xs">End Date</span></label>
                    <input
                        type="date"
                        class="input input-bordered input-xs"
                        on:input=move |ev| set_end_date.set(event_target_value(&ev))
                        prop:value=move || end_date.get()
                        required
                    />
                </div>
                <div class="form-control">
                    <label class="label py-1"><span class="label-text text-xs">Nightly Rate ($)</span></label>
                    <input
                        type="number"
                        step="0.01"
                        placeholder="250.00"
                        class="input input-bordered input-xs w-24"
                        on:input=move |ev| set_nightly_rate.set(event_target_value(&ev))
                        prop:value=move || nightly_rate.get()
                        required
                    />
                </div>
                <div class="form-control">
                    <label class="label py-1"><span class="label-text text-xs">Min Nights</span></label>
                    <input
                        type="number"
                        min="1"
                        class="input input-bordered input-xs w-20"
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                                set_min_nights.set(v);
                            }
                        }
                        prop:value=move || min_nights.get()
                        required
                    />
                </div>
                <button type="submit" class="btn btn-primary btn-xs" prop:disabled=move || loading.get()>
                    "Add Rate Override"
                </button>
            </form>
        </div>
    }
}

#[server]
pub async fn submit_host_reply_action(
    review_id: String,
    reply_text: String,
) -> Result<(), ServerFnError> {
    let review_uuid =
        uuid::Uuid::parse_str(&review_id).map_err(|e| ServerFnError::new(e.to_string()))?;
    let req = common::models::HostReplyRequest { reply_text };
    web_app_common::reviews::submit_host_reply_server(review_uuid, req).await
}

#[component]
#[allow(non_snake_case)]
pub fn ListingReviewsAdminSection(listing_id: String, listing_name: String) -> impl IntoView {
    let lid = uuid::Uuid::parse_str(&listing_id).unwrap_or_default();

    let (refresh, set_refresh) = signal(0);

    let reviews_resource = Resource::new(
        move || (lid, refresh.get()),
        |(id, _)| async move { web_app_common::reviews::get_listing_reviews_server(id, 1, 50).await },
    );

    view! {
        <div class="mt-4 p-4 border border-base-300 rounded-box bg-base-200 space-y-4">
            <h3 class="font-bold text-md text-primary">"Guest Reviews — " {listing_name}</h3>

            <Suspense fallback=move || view! { <div class="loading loading-spinner"></div> }>
                {move || reviews_resource.get().map(|res| match res {
                    Ok(reviews) => {
                        if reviews.is_empty() {
                            view! { <p class="text-sm text-base-content/70">"No reviews yet."</p> }.into_any()
                        } else {
                            view! {
                                <div class="grid grid-cols-1 gap-4">
                                    <For
                                        each=move || reviews.clone()
                                        key=|review| review.id
                                        children=move |review| {
                                            view! {
                                                <AdminReviewCard review=review set_refresh=set_refresh />
                                            }
                                        }
                                    />
                                </div>
                            }.into_any()
                        }
                    }
                    Err(e) => view! { <div class="alert alert-error"><span>{e.to_string()}</span></div> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

#[component]
fn AdminReviewCard(
    review: common::models::ReviewResponse,
    set_refresh: WriteSignal<i32>,
) -> impl IntoView {
    let review_id = review.id;
    let submit_reply = ServerAction::<SubmitHostReplyAction>::new();
    let (show_reply_form, set_show_reply_form) = signal(false);

    Effect::new(move || {
        if let Some(Ok(_)) = submit_reply.value().get() {
            set_show_reply_form.set(false);
            set_refresh.update(|v| *v += 1);
        }
    });

    view! {
        <div class="card bg-base-100 shadow-sm border border-base-200 p-4">
            <div class="flex justify-between items-start">
                <div>
                    <div class="font-bold">{review.guest_first_name.clone()}</div>
                    <div class="text-xs text-base-content/60">{review.created_at.format("%b %d, %Y").to_string()}</div>
                </div>
                <div class="badge badge-primary">{format!("{:.1}", review.overall_rating)}</div>
            </div>
            <div class="mt-2 text-sm whitespace-pre-line">
                {review.public_review_text.clone().unwrap_or_default()}
            </div>

            <div class="mt-4 pt-4 border-t border-base-200">
                {
                    let r = review.clone();
                    move || {
                        if let Some(reply) = r.host_reply_text.clone() {
                            view! {
                                <div class="bg-base-200 p-3 rounded-lg">
                                    <div class="font-semibold text-xs mb-1">"Your Reply:"</div>
                                    <div class="text-sm">{reply}</div>
                                </div>
                            }.into_any()
                        } else if show_reply_form.get() {
                            view! {
                                <ActionForm action=submit_reply>
                                    <div class="flex flex-col gap-2">
                                        <input type="hidden" name="review_id" value=review_id.to_string() />
                                        <textarea
                                            name="reply_text"
                                            class="textarea textarea-bordered textarea-sm w-full"
                                            placeholder="Write a public reply to this review..."
                                            required
                                        ></textarea>
                                        <div class="flex gap-2 justify-end">
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs"
                                                on:click=move |_| set_show_reply_form.set(false)
                                            >
                                                "Cancel"
                                            </button>
                                            <button
                                                type="submit"
                                                class="btn btn-primary btn-xs"
                                                disabled=move || submit_reply.pending().get()
                                            >
                                                "Submit Reply"
                                            </button>
                                        </div>
                                    </div>
                                </ActionForm>
                            }.into_any()
                        } else {
                            view! {
                                <button
                                    class="btn btn-outline btn-xs"
                                    on:click=move |_| set_show_reply_form.set(true)
                                >
                                    "Reply to Review"
                                </button>
                            }.into_any()
                        }
                    }
                }
            </div>
        </div>
    }
}
