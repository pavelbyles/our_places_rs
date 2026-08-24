use crate::app::AuthContext;
use crate::auth::UserProfile;
use chrono::NaiveDate;
use common::models::ListingResponse;
use leptos::prelude::*;
use num_format::{Locale, ToFormattedString};
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckoutDetails {
    pub booking: common::models::BookingResponse,
    pub listing: ListingResponse,
}

#[server]
pub async fn initiate_booking(
    listing_id: Uuid,
    check_in: NaiveDate,
    check_out: NaiveDate,
    adults: u32,
    children: u32,
    infants: u32,
    pets: u32,
) -> Result<Uuid, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use actix_session::Session;
        use common::models::NewBookingRequest;
        use web_app_common::bookings::create_booking_api;

        let session = leptos_actix::extract::<Session>().await?;
        let user_currency = session
            .get::<String>("user_default_currency")
            .ok()
            .flatten()
            .unwrap_or_else(|| "USD".to_string());

        // 1. Determine Guest ID
        let guest_id = if let Some(user_id_str) = session.get::<String>("user_id").ok().flatten() {
            Uuid::parse_str(&user_id_str)
                .map_err(|e| ServerFnError::new(format!("Invalid session: {}", e)))?
        } else {
            // For shadow / unauthenticated checkout, generate guest UUID v7
            Uuid::now_v7()
        };

        let req = NewBookingRequest {
            guest_id,
            listing_id,
            check_in,
            check_out,
            num_adults: adults,
            num_children: children,
            num_infants: infants,
            num_pets: pets,
            message_to_host: None,
            estimated_arrival_time: None,
            is_business_trip: false,
            currency: user_currency,
            agreed_cancellation_policy: "flexible".to_string(),
        };

        let booking = create_booking_api(req).await?;

        if session.get::<String>("user_id").ok().flatten().is_none() {
            session
                .insert("pending_booking_id", booking.id.to_string())
                .ok();
        }

        Ok(booking.id)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (
            listing_id, check_in, check_out, adults, children, infants, pets,
        );
        Ok(Uuid::nil())
    }
}

#[server]
pub async fn claim_pending_booking(booking_id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use actix_session::Session;
        use web_app_common::bookings::transfer_booking_api;

        let session = leptos_actix::extract::<Session>().await?;
        let user_id_str = session
            .get::<String>("user_id")
            .ok()
            .flatten()
            .ok_or_else(|| ServerFnError::new("Unauthorized"))?;

        let user_id =
            Uuid::parse_str(&user_id_str).map_err(|_| ServerFnError::new("Invalid user ID"))?;

        transfer_booking_api(booking_id, user_id).await?;

        session.remove("pending_booking_id");
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = booking_id;
        Ok(())
    }
}

#[server]
pub async fn get_checkout_data(booking_id: Uuid) -> Result<CheckoutDetails, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use actix_session::Session;
        use web_app_common::bookings::get_booking_by_id_api;
        use web_app_common::listings::get_listing_by_id_server;

        let session = leptos_actix::extract::<Session>().await.ok();
        let currency = session
            .as_ref()
            .and_then(|s| s.get::<String>("user_default_currency").ok().flatten());

        let booking = get_booking_by_id_api(booking_id, currency.clone()).await?;
        let listing_details =
            get_listing_by_id_server(booking.listing_id.to_string(), currency).await?;

        Ok(CheckoutDetails {
            booking,
            listing: listing_details.listing,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = booking_id;
        Err(ServerFnError::new("Not implemented on client"))
    }
}

#[server]
pub async fn complete_booking(
    booking_id: Uuid,
    email: String,
    full_name: String,
    _phone: String,
    metadata: common::models::BookingMetadataResponse,
) -> Result<common::models::BookingResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use common::models::UpdatedBookingRequest;
        use web_app_common::bookings::update_booking_api;
        use web_app_common::email::send_booking_confirmation;
        use web_app_common::listings::get_listing_by_id_server;

        tracing::info!("Completing booking for ID: {}", booking_id);

        let update_req = UpdatedBookingRequest {
            status: Some("confirmed".to_string()),
            metadata: Some(metadata),
        };

        let booking = update_booking_api(booking_id, update_req).await?;
        let listing_details =
            get_listing_by_id_server(booking.listing_id.to_string(), None).await?;

        let first_name = full_name
            .split_whitespace()
            .next()
            .unwrap_or(&full_name)
            .to_string();

        send_booking_confirmation(
            &email,
            &first_name,
            &listing_details.listing.name,
            &booking.confirmation_code,
            &booking.date_from.to_string(),
            &booking.date_to.to_string(),
            booking.number_of_persons,
        )
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to send confirmation email: {}", e)))?;

        tracing::info!(
            "Booking {} successfully confirmed and email sent.",
            booking_id
        );
        Ok(booking)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (booking_id, email, full_name, _phone, metadata);
        Err(ServerFnError::new("Not implemented on client"))
    }
}

#[server]
pub async fn cancel_booking(booking_id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use actix_session::Session;
        use common::models::UpdatedBookingRequest;
        use web_app_common::bookings::{
            delete_booking_api, get_booking_by_id_api, update_booking_api,
        };

        let session = leptos_actix::extract::<Session>().await?;

        // 1. Fetch booking details from booking_api
        let booking = get_booking_by_id_api(booking_id, None).await?;

        // 2. Authorization check:
        let is_authorized = if let Some(user_id_str) =
            session.get::<String>("user_id").ok().flatten()
        {
            if let Ok(user_id) = Uuid::parse_str(&user_id_str) {
                booking.guest_id == user_id
            } else {
                false
            }
        } else if let Some(pending_id) = session.get::<String>("pending_booking_id").ok().flatten()
        {
            pending_id == booking_id.to_string() && booking.status.eq_ignore_ascii_case("pending")
        } else {
            false
        };

        if !is_authorized {
            return Err(ServerFnError::new(
                "Unauthorized: You do not have permission to cancel this booking",
            ));
        }

        // 3. Handle according to status
        let status_lower = booking.status.to_lowercase();
        if status_lower == "pending" {
            // Delete the in-flight hold via HTTP DELETE to booking_api
            delete_booking_api(booking_id).await?;
        } else if status_lower == "confirmed" {
            // Transition confirmed booking to cancelled via HTTP PATCH to booking_api
            let update_req = UpdatedBookingRequest {
                status: Some("cancelled".to_string()),
                metadata: None,
            };
            update_booking_api(booking_id, update_req).await?;
        } else if status_lower == "completed" {
            return Err(ServerFnError::new("Cannot cancel a completed booking"));
        }

        // 4. Session cleanup
        session.remove("pending_booking_id");

        leptos_actix::redirect(&format!("/listing/{}", booking.listing_id));
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = booking_id;
        Ok(())
    }
}

#[component]
pub fn CheckoutPage() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let booking_id = move || {
        params.with(|p| {
            p.get("id")
                .and_then(|id| Uuid::parse_str(&id).ok())
                .unwrap_or_default()
        })
    };

    let auth = use_context::<AuthContext>().expect("AuthContext required");
    let user_resource = auth.user;

    let checkout_data = Resource::new(booking_id, |id| async move { get_checkout_data(id).await });

    let email = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let phone = RwSignal::new(String::new());

    let message_to_host = RwSignal::new(String::new());
    let is_business_trip = RwSignal::new(false);
    let arrival_time = RwSignal::new(String::new());

    // Effect to prefill
    Effect::new(move |_| {
        if let Some(Ok(data)) = checkout_data.get() {
            let booking = data.booking;
            message_to_host.set(booking.metadata.message_to_host.unwrap_or_default());
            is_business_trip.set(booking.metadata.is_business_trip);
            arrival_time.set(booking.metadata.estimated_arrival_time.unwrap_or_default());
        }

        if let Some(Ok(Some(user))) = user_resource.get() {
            email.set(user.email.clone());
            name.set(user.name.clone());
            phone.set(user.phone_number.clone().unwrap_or_default());
        }
    });

    let complete_booking_action = Action::new(
        move |(id, email, name, phone, metadata): &(
            Uuid,
            String,
            String,
            String,
            common::models::BookingMetadataResponse,
        )| {
            let id = *id;
            let email = email.clone();
            let name = name.clone();
            let phone = phone.clone();
            let metadata = metadata.clone();
            async move { complete_booking(id, email, name, phone, metadata).await }
        },
    );

    let cancel_booking_action = Action::new(move |id: &Uuid| {
        let id = *id;
        async move { cancel_booking(id).await }
    });

    let confirmed_booking =
        RwSignal::new(Option::<(common::models::BookingResponse, ListingResponse, String)>::None);

    Effect::new(move || {
        if let Some(Ok(booking)) = complete_booking_action.value().get() {
            if let Some(Ok(data)) = checkout_data.get() {
                confirmed_booking.set(Some((booking, data.listing.clone(), email.get())));
            }
        }
    });

    Effect::new(move || {
        if let Some(Ok(())) = cancel_booking_action.value().get() {
            if let Some(Ok(data)) = checkout_data.get() {
                leptos_router::hooks::use_navigate()(
                    &format!("/listing/{}", data.listing.id),
                    Default::default(),
                );
            } else {
                leptos_router::hooks::use_navigate()("/", Default::default());
            }
        }
    });

    view! {
        <div class="container mx-auto px-4 py-12 max-w-6xl">
            // Booking Confirmation Modal & Confetti Popup
            {move || {
                if let Some((booking, listing, guest_email)) = confirmed_booking.get() {
                    view! {
                        <BookingConfirmationModal
                            booking=booking
                            listing=listing
                            email=guest_email
                        />
                    }.into_any()
                } else {
                    view! { <span class="hidden"></span> }.into_any()
                }
            }}

            <h1 class="text-3xl font-bold mb-8">"Confirm and Pay"</h1>

            <Suspense fallback=move || view! { <div class="loading loading-spinner loading-lg mx-auto block"></div> }>
                {move || checkout_data.get().map(|res| match res {
                    Ok(data) => {
                        let data = data.clone();
                        let booking = data.booking.clone();
                        let listing = data.listing.clone();

                        view! {
                            <div class="grid grid-cols-1 lg:grid-cols-2 gap-12">
                                // Left Column: Form
                                <div class="space-y-8">
                                    <section>
                                        <h2 class="text-2xl font-semibold mb-4">"Your Trip"</h2>
                                        <div class="flex justify-between items-center py-4 border-b">
                                            <div>
                                                <p class="font-bold">"Dates"</p>
                                                <p class="text-base-content/70">{booking.date_from.to_string()} " – " {booking.date_to.to_string()}</p>
                                            </div>
                                        </div>
                                        <div class="flex justify-between items-center py-4 border-b">
                                            <div>
                                                <p class="font-bold">"Guests"</p>
                                                <p class="text-base-content/70">{booking.number_of_persons} " guest" {if booking.number_of_persons == 1 { "" } else { "s" }}</p>
                                            </div>
                                        </div>
                                    </section>

                                    <section>
                                        <h2 class="text-2xl font-semibold mb-4">"Cancellation Policy"</h2>
                                        <p class="text-base-content/70">
                                            {booking.cancellation_policy.clone()} ". Free cancellation for 48 hours. After that, cancel before check-in for a partial refund."
                                        </p>
                                    </section>

                                    <section>
                                        <h2 class="text-2xl font-semibold mb-4">"Contact Information"</h2>
                                        <ContactForm
                                            user_resource=user_resource
                                            email=email
                                            name=name
                                            phone=phone
                                        />
                                    </section>

                                    <section>
                                        <h2 class="text-2xl font-semibold mb-4">"Additional Details"</h2>
                                        <div class="space-y-4">
                                            <div class="form-control">
                                                <label class="label cursor-pointer justify-start gap-4">
                                                    <input type="checkbox"
                                                        class="checkbox checkbox-primary"
                                                        checked=is_business_trip
                                                        on:change:target=move |ev| is_business_trip.set(ev.target().checked())
                                                    />
                                                    <span class="label-text">"This is a business trip"</span>
                                                </label>
                                            </div>

                                            <fieldset class="fieldset w-full">
                                                <legend class="fieldset-legend font-bold">"Message to host (optional)"</legend>
                                                <textarea
                                                    class="textarea h-24 w-full"
                                                    placeholder="Tell the host about your trip..."
                                                    on:input:target=move |ev| message_to_host.set(ev.target().value())
                                                    prop:value=message_to_host
                                                ></textarea>
                                            </fieldset>

                                            <fieldset class="fieldset w-full">
                                                <legend class="fieldset-legend font-bold">"Estimated Arrival Time (optional)"</legend>
                                                <select class="select w-full"
                                                    on:change:target=move |ev| arrival_time.set(ev.target().value())
                                                    prop:value=arrival_time
                                                >
                                                    <option value="">"Select a time"</option>
                                                    <option value="09:00">"09:00 AM – 11:00 AM"</option>
                                                    <option value="11:00">"11:00 AM – 01:00 PM"</option>
                                                    <option value="13:00">"01:00 PM – 03:00 PM"</option>
                                                    <option value="15:00">"03:00 PM – 05:00 PM"</option>
                                                    <option value="17:00">"05:00 PM – 07:00 PM"</option>
                                                    <option value="19:00">"07:00 PM – 12:00 AM"</option>
                                                </select>
                                            </fieldset>
                                        </div>
                                    </section>

                                    <div class="pt-8 flex flex-col sm:flex-row gap-4">
                                        <button
                                            type="button"
                                            class="btn btn-outline btn-error btn-lg flex-1 order-2 sm:order-1"
                                            disabled=move || cancel_booking_action.pending().get() || complete_booking_action.pending().get()
                                            on:click=move |_| {
                                                cancel_booking_action.dispatch(booking_id());
                                            }
                                        >
                                            {move || if cancel_booking_action.pending().get() {
                                                view! { <span class="loading loading-spinner"></span> }.into_any()
                                            } else {
                                                view! { "Cancel Reservation" }.into_any()
                                            }}
                                        </button>
                                        <button
                                            type="button"
                                            class="btn btn-primary btn-lg flex-1 order-1 sm:order-2"
                                            disabled=move || complete_booking_action.pending().get() || cancel_booking_action.pending().get()
                                            on:click=move |_| {
                                                if let Some(Ok(data)) = checkout_data.get() {
                                                    let meta = common::models::BookingMetadataResponse {
                                                        num_adults: data.booking.metadata.num_adults,
                                                        num_children: data.booking.metadata.num_children,
                                                        num_infants: data.booking.metadata.num_infants,
                                                        num_pets: data.booking.metadata.num_pets,
                                                        message_to_host: Some(message_to_host.get()).filter(|s| !s.is_empty()),
                                                        estimated_arrival_time: Some(arrival_time.get()).filter(|s| !s.is_empty()),
                                                        is_business_trip: is_business_trip.get(),
                                                    };
                                                    complete_booking_action.dispatch((booking_id(), email.get(), name.get(), phone.get(), meta));
                                                }
                                            }
                                        >
                                            {move || if complete_booking_action.pending().get() {
                                                view! { <span class="loading loading-spinner"></span> }.into_any()
                                            } else {
                                                view! { "Confirm Booking" }.into_any()
                                            }}
                                        </button>
                                    </div>
                                </div>

                                // Right Column: Summary
                                <div>
                                    <div class="card bg-base-100 shadow-xl border border-base-200 sticky top-8">
                                        <div class="card-body p-6">
                                            <div class="flex gap-4 mb-6 pb-6 border-b">
                                                <div class="w-24 h-24 rounded-lg bg-base-300 overflow-hidden flex-shrink-0">
                                                    <img src=listing.primary_image_url.clone().unwrap_or_else(|| "https://images.unsplash.com/photo-1512917774080-9991f1c4c750?auto=format&fit=crop&w=800&q=80".to_string()) alt="Listing" class="w-full h-full object-cover" />
                                                </div>
                                                <div>
                                                    <p class="text-xs text-base-content/60 uppercase font-bold">{listing.listing_structure.clone()}</p>
                                                    <p class="text-lg font-semibold leading-tight">{listing.name.clone()}</p>
                                                    <div class="flex items-center gap-1 mt-1">
                                                        <i class="fa-solid fa-star text-xs"></i>
                                                        <span class="text-sm font-bold">"4.92"</span>
                                                        <span class="text-sm text-base-content/60">"(128 reviews)"</span>
                                                    </div>
                                                </div>
                                            </div>

                                            <h3 class="text-xl font-bold mb-4">"Price details"</h3>
                                            <div class="space-y-3">
                                                <div class="flex justify-between">
                                                    <span>{booking.currency.clone()} " " {listing.price_per_night.unwrap_or_default().to_i64().unwrap_or_default().to_formatted_string(&Locale::en)} " x " {booking.total_days} " nights"</span>
                                                    <span>{booking.currency.clone()} " " {booking.sub_total_price.to_i64().unwrap_or_default().to_formatted_string(&Locale::en)}</span>
                                                </div>
                                                <div class="flex justify-between">
                                                    <span class="underline">"Service fee"</span>
                                                    <span>{booking.currency.clone()} " " {booking.tax_value.unwrap_or_default().to_i64().unwrap_or_default().to_formatted_string(&Locale::en)}</span>
                                                </div>
                                                <div class="flex justify-between font-bold text-lg pt-4 border-t border-base-200">
                                                    <span>"Total (" {booking.currency.clone()} ")"</span>
                                                    <span>{booking.currency.clone()} " " {booking.total_price.to_i64().unwrap_or_default().to_formatted_string(&Locale::en)}</span>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        }.into_any()
                    }
                    Err(e) => view! {
                        <div class="alert alert-error">
                            <span>"Error loading checkout: " {e.to_string()}</span>
                        </div>
                    }.into_any()
                })}
            </Suspense>

            {move || complete_booking_action.value().get().map(|res| match res {
                Err(e) => view! {
                    <div class="alert alert-error mt-4">
                        <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                        <span>"Failed to complete booking: " {e.to_string()}</span>
                    </div>
                }.into_any(),
                _ => view! { <div></div> }.into_any()
            })}

            {move || cancel_booking_action.value().get().map(|res| match res {
                Err(e) => view! {
                    <div class="alert alert-error mt-4">
                        <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                        <span>"Failed to cancel booking: " {e.to_string()}</span>
                    </div>
                }.into_any(),
                _ => view! { <div></div> }.into_any()
            })}
        </div>
    }
}

#[component]
fn ContactForm(
    user_resource: Resource<Result<Option<UserProfile>, ServerFnError>>,
    email: RwSignal<String>,
    name: RwSignal<String>,
    phone: RwSignal<String>,
) -> impl IntoView {
    view! {
        <Suspense fallback=move || view! { <div class="loading loading-sm"></div> }>
            {move || user_resource.get().map(|res| match res {
                Ok(Some(_)) => view! {
                    <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
                        <fieldset class="fieldset">
                            <legend class="fieldset-legend">"Email"</legend>
                            <input type="email"
                                on:input:target=move |ev| email.set(ev.target().value())
                                value=email class="input w-full" readonly />
                        </fieldset>
                        <fieldset class="fieldset">
                            <legend class="fieldset-legend">"Full Name"</legend>
                            <input type="text"
                                on:input:target=move |ev| name.set(ev.target().value())
                                value=name class="input w-full" />
                        </fieldset>
                        <fieldset class="fieldset md:col-span-2">
                            <legend class="fieldset-legend">"Phone Number"</legend>
                            <input type="tel"
                                on:input:target=move |ev| phone.set(ev.target().value())
                                value=phone placeholder="+1 (555) 000-0000" class="input w-full" />
                        </fieldset>
                    </div>
                }.into_any(),
                _ => view! {
                    <div class="space-y-4">
                        <div class="alert bg-base-200 border-none">
                            <i class="fa-solid fa-circle-info text-info"></i>
                            <div>
                                <h3 class="font-bold">"Sign in to book faster"</h3>
                                <div class="text-xs">"Access your saved details and trip history."</div>
                            </div>
                            <div class="flex flex-col sm:flex-row gap-2 mt-2 sm:mt-0 xl:ml-auto">
                                <a href="/login" class="btn btn-sm btn-ghost">"Log in"</a>
                                <a href="/register" class="btn btn-sm btn-primary">"Sign up"</a>
                            </div>
                        </div>
                        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                            <fieldset class="fieldset">
                                <legend class="fieldset-legend">"Email"</legend>
                                <input type="email"
                                    on:input:target=move |ev| email.set(ev.target().value())
                                    prop:value=email
                                    placeholder="Your email address" class="input w-full" />
                            </fieldset>
                            <fieldset class="fieldset">
                                <legend class="fieldset-legend">"Full Name"</legend>
                                <input type="text"
                                    on:input:target=move |ev| name.set(ev.target().value())
                                    prop:value=name
                                    placeholder="As it appears on ID" class="input w-full" />
                            </fieldset>
                            <fieldset class="fieldset md:col-span-2">
                                <legend class="fieldset-legend">"Phone Number"</legend>
                                <input type="tel"
                                    on:input:target=move |ev| phone.set(ev.target().value())
                                    prop:value=phone
                                    placeholder="+1 (555) 000-0000" class="input w-full" />
                            </fieldset>
                        </div>
                    </div>
                }.into_any()
            })}
        </Suspense>
    }
}

#[component]
fn ConfettiCelebration() -> impl IntoView {
    let particles = [
        (4, 50, "3.1s", "1.2s", "#FF4757", 10, 0, 45),
        (8, 200, "2.8s", "1.5s", "#2ED573", 8, 1, 0),
        (12, 400, "3.6s", "1.1s", "#1E90FF", 12, 2, -30),
        (16, 100, "3.0s", "1.7s", "#FFA502", 9, 0, 60),
        (20, 300, "3.4s", "1.3s", "#9B59B6", 11, 1, 0),
        (24, 50, "2.9s", "1.6s", "#FF6B81", 8, 2, 20),
        (28, 500, "3.8s", "1.0s", "#70A1FF", 13, 0, -45),
        (32, 150, "3.2s", "1.4s", "#ECCC68", 10, 1, 0),
        (36, 350, "3.5s", "1.8s", "#00D2D3", 7, 2, 75),
        (40, 0, "2.7s", "1.2s", "#FF4757", 12, 0, -15),
        (44, 250, "3.3s", "1.5s", "#5352ED", 9, 1, 0),
        (48, 450, "3.7s", "1.1s", "#2ED573", 11, 2, 40),
        (52, 100, "3.0s", "1.6s", "#FFA502", 8, 0, -60),
        (56, 300, "3.4s", "1.3s", "#FF6B81", 10, 1, 0),
        (60, 50, "2.8s", "1.7s", "#1E90FF", 12, 2, 15),
        (64, 400, "3.6s", "1.0s", "#9B59B6", 7, 0, 90),
        (68, 150, "3.1s", "1.4s", "#ECCC68", 11, 1, 0),
        (72, 350, "3.5s", "1.9s", "#00D2D3", 9, 2, -25),
        (76, 50, "2.9s", "1.2s", "#FF4757", 13, 0, 35),
        (80, 250, "3.3s", "1.5s", "#2ED573", 8, 1, 0),
        (84, 450, "3.7s", "1.1s", "#70A1FF", 10, 2, -70),
        (88, 100, "3.0s", "1.6s", "#FFA502", 12, 0, 50),
        (92, 300, "3.4s", "1.3s", "#FF6B81", 7, 1, 0),
        (96, 200, "2.8s", "1.7s", "#5352ED", 11, 2, -10),
        (6, 600, "3.2s", "1.4s", "#2ED573", 9, 0, 30),
        (14, 750, "3.5s", "1.2s", "#FF4757", 11, 1, 0),
        (22, 650, "2.9s", "1.6s", "#FFA502", 8, 2, -45),
        (30, 850, "3.7s", "1.1s", "#1E90FF", 12, 0, 60),
        (38, 700, "3.0s", "1.5s", "#9B59B6", 10, 1, 0),
        (46, 900, "3.6s", "1.3s", "#00D2D3", 7, 2, 15),
        (54, 600, "2.8s", "1.8s", "#FF6B81", 13, 0, -30),
        (62, 800, "3.4s", "1.0s", "#ECCC68", 9, 1, 0),
        (70, 700, "3.1s", "1.4s", "#5352ED", 11, 2, 75),
        (78, 950, "3.8s", "1.2s", "#2ED573", 8, 0, -60),
        (86, 650, "2.9s", "1.6s", "#FF4757", 10, 1, 0),
        (94, 850, "3.5s", "1.5s", "#70A1FF", 12, 2, 45),
        (10, 1100, "3.3s", "1.3s", "#ECCC68", 10, 0, -15),
        (26, 1200, "3.6s", "1.5s", "#00D2D3", 8, 1, 0),
        (42, 1050, "3.0s", "1.7s", "#FF6B81", 11, 2, 60),
        (58, 1150, "3.4s", "1.1s", "#9B59B6", 9, 0, -45),
        (74, 1250, "3.7s", "1.4s", "#1E90FF", 12, 1, 0),
        (90, 1100, "3.1s", "1.6s", "#2ED573", 7, 2, 30),
    ];

    view! {
        <div class="fixed inset-0 pointer-events-none z-50 overflow-hidden" aria-hidden="true">
            {particles.into_iter().map(|(left, delay, dur, wobble, color, size, shape, rotate)| {
                let style = format!(
                    "left: {}%; --fall-delay: {}ms; --fall-duration: {}; --wobble-duration: {}; background-color: {}; width: {}px; height: {}px; transform: rotate({}deg);",
                    left, delay, dur, wobble, color, size, if shape == 2 { size * 2 } else { size }, rotate
                );
                let shape_class = match shape {
                    1 => "rounded-full",
                    2 => "rounded-xs",
                    _ => "rounded-sm",
                };
                view! {
                    <div
                        class=format!("absolute top-0 animate-confetti-particle shadow-sm {}", shape_class)
                        style=style
                    ></div>
                }
            }).collect_view()}
        </div>
    }
}

#[component]
fn BookingConfirmationModal(
    booking: common::models::BookingResponse,
    listing: common::models::ListingResponse,
    email: String,
) -> impl IntoView {
    let copied = RwSignal::new(false);
    let conf_code = booking.confirmation_code.clone();

    let copy_code = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                let _ = window.navigator().clipboard().write_text(&conf_code);
                copied.set(true);
                leptos::prelude::set_timeout(
                    move || copied.set(false),
                    std::time::Duration::from_secs(2),
                );
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = &conf_code;
            copied.set(true);
        }
    };

    view! {
        <ConfettiCelebration />

        <div class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-fade-in">
            <div class="card bg-base-100 max-w-lg w-full shadow-2xl border border-base-200 animate-modal-pop relative overflow-hidden">
                // Decorative Top Accent Bar
                <div class="h-2 w-full bg-gradient-to-r from-emerald-400 via-primary to-secondary"></div>

                <div class="card-body p-6 sm:p-8 text-center items-center">
                    // Success Burst Icon
                    <div class="relative mb-3">
                        <div class="w-16 h-16 rounded-full bg-success/20 flex items-center justify-center animate-ping absolute inset-0 opacity-40"></div>
                        <div class="w-16 h-16 rounded-full bg-success text-success-content flex items-center justify-center shadow-lg relative animate-checkmark-burst">
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-9 w-9" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                            </svg>
                        </div>
                    </div>

                    // Heading & Subheading
                    <h2 class="text-2xl sm:text-3xl font-black text-base-content tracking-tight">
                        "Booking Confirmed! 🎉"
                    </h2>
                    <p class="text-sm text-base-content/70 mt-1 max-w-sm">
                        "Pack your bags! Your luxury getaway in Jamaica is officially confirmed."
                    </p>

                    // Booking Snapshot Box
                    <div class="w-full bg-base-200/60 rounded-2xl p-4 mt-5 text-left border border-base-300/60 space-y-3.5">
                        // Listing Details Preview
                        <div class="flex items-center gap-3 pb-3 border-b border-base-300/60">
                            <div class="w-14 h-14 rounded-xl bg-base-300 overflow-hidden flex-shrink-0 shadow-inner">
                                <img
                                    src=listing.primary_image_url.clone().unwrap_or_else(|| "https://images.unsplash.com/photo-1512917774080-9991f1c4c750?auto=format&fit=crop&w=800&q=80".to_string())
                                    alt=listing.name.clone()
                                    class="w-full h-full object-cover"
                                />
                            </div>
                            <div class="min-w-0 flex-1">
                                <p class="text-[11px] font-bold uppercase tracking-wider text-primary truncate">
                                    {listing.listing_structure.clone()}
                                </p>
                                <h3 class="font-bold text-sm sm:text-base text-base-content leading-snug line-clamp-1">
                                    {listing.name.clone()}
                                </h3>
                                <p class="text-xs text-base-content/60">
                                    {listing.city.clone().unwrap_or_else(|| "Jamaica".to_string())} ", " {listing.country.clone()}
                                </p>
                            </div>
                        </div>

                        // Confirmation Code with Copy Button
                        <div class="flex items-center justify-between bg-base-100 rounded-xl px-3.5 py-2.5 border border-base-300/80">
                            <div>
                                <p class="text-[10px] uppercase font-bold text-base-content/50 tracking-wider">"Confirmation Code"</p>
                                <p class="font-mono font-black text-base sm:text-lg text-primary tracking-wider">{booking.confirmation_code.clone()}</p>
                            </div>
                            <button
                                type="button"
                                class=move || format!("btn btn-xs sm:btn-sm gap-1.5 transition-all {}", if copied.get() { "btn-success text-white" } else { "btn-ghost border border-base-300" })
                                on:click=copy_code
                            >
                                {move || if copied.get() {
                                    view! {
                                        <>
                                            <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M5 13l4 4L19 7" />
                                            </svg>
                                            <span>"Copied!"</span>
                                        </>
                                    }.into_any()
                                } else {
                                    view! {
                                        <>
                                            <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                                            </svg>
                                            <span>"Copy"</span>
                                        </>
                                    }.into_any()
                                }}
                            </button>
                        </div>

                        // Dates, Guests, and Total Grid
                        <div class="grid grid-cols-2 gap-2 text-xs">
                            <div class="bg-base-100/70 p-2.5 rounded-xl border border-base-300/50">
                                <span class="text-base-content/60 block text-[10px] uppercase font-bold">"Dates"</span>
                                <span class="font-bold text-base-content">{format!("{} – {}", booking.date_from, booking.date_to)}</span>
                                <span class="text-base-content/50 block text-[11px]">{format!("{} nights", booking.total_days)}</span>
                            </div>
                            <div class="bg-base-100/70 p-2.5 rounded-xl border border-base-300/50">
                                <span class="text-base-content/60 block text-[10px] uppercase font-bold">"Total Paid"</span>
                                <span class="font-extrabold text-base sm:text-lg text-emerald-600 dark:text-emerald-400">
                                    {format!("{} {:.2}", booking.currency, booking.total_price)}
                                </span>
                            </div>
                        </div>
                    </div>

                    // Email Receipt Notice
                    <div class="flex items-center gap-2 mt-4 text-xs text-base-content/70">
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-primary shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 8l7.89 5.26a2 2 0 002.22 0L21 8M5 19h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                        </svg>
                        <span>"Receipt & check-in details sent to " <strong class="text-base-content">{email}</strong></span>
                    </div>

                    // Action CTAs
                    <div class="flex flex-col sm:flex-row gap-3 w-full mt-6">
                        <a
                            href="/bookings"
                            class="btn btn-primary flex-1 gap-2 text-sm shadow-md"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                            </svg>
                            "View My Bookings"
                        </a>
                        <a
                            href="/"
                            class="btn btn-ghost border border-base-300 flex-1 text-sm"
                        >
                            "Explore More"
                        </a>
                    </div>
                </div>
            </div>
        </div>
    }
}
