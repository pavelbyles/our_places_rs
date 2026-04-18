use crate::app::AuthContext;
use crate::auth::UserProfile;
use chrono::NaiveDate;
use common::models::ListingResponse;
use leptos::prelude::*;
use num_format::{Locale, ToFormattedString};
#[cfg(feature = "ssr")]
use rust_decimal::Decimal;
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
        use db_core::booking as db_booking;
        use db_core::listing as db_listing;
        use db_core::models::{BookingMetadata, NewBooking, NewUser};
        use db_core::user as db_user;
        use rand::RngExt;
        use web_app_common::api_client::get_pool;

        let pool = get_pool().await;
        let session = leptos_actix::extract::<Session>().await?;

        // 1. Determine Guest ID
        let guest_id = if let Some(user_id_str) = session.get::<String>("user_id").ok().flatten() {
            Uuid::parse_str(&user_id_str)
                .map_err(|e| ServerFnError::new(format!("Invalid session: {}", e)))?
        } else {
            // Create a dummy guest
            let mut rng = rand::rng();
            let guest_num: u32 = rng.random_range(100000..999999);
            let guest_id = Uuid::now_v7();
            let dummy_email = format!("guest_{}@ourplaces.io", guest_num);

            let new_user = NewUser {
                id: guest_id,
                email: dummy_email,
                password_hash: "GUEST_PLACEHOLDER".to_string(),
                first_name: "Guest".to_string(),
                last_name: guest_num.to_string(),
                phone_number: None,
                is_active: true,
                is_verified: false,
                verification_code: None,
                verification_code_expires_at: None,
                attributes: serde_json::json!({"is_guest": true}),
                roles: Some(vec![db_core::models::UserRole::Booker]),
            };

            db_user::create_user(&pool, &new_user)
                .await
                .map_err(|e| ServerFnError::new(format!("Failed to create guest: {}", e)))?;
            guest_id
        };

        // 2. Fetch Listing info for price/policy
        let listing_details = db_listing::get_listing_by_id(&pool, listing_id)
            .await
            .map_err(|e| ServerFnError::new(format!("Listing not found: {}", e)))?;

        let listing = listing_details.listing;
        let total_days = (check_out - check_in).num_days() as i32;
        let daily_rate = listing.price_per_night.unwrap_or(Decimal::ZERO);
        let sub_total_price = daily_rate * Decimal::from(total_days);

        // Simple fees/tax for now
        let tax_value = sub_total_price * Decimal::new(1, 1); // 10%
        let total_price = sub_total_price + tax_value;

        let confirmation_code = (0..8)
            .map(|_| {
                let idx = rand::rng().random_range(0..26);
                (b'A' + idx as u8) as char
            })
            .collect::<String>();

        let new_booking = NewBooking {
            confirmation_code,
            guest_id,
            listing_id,
            date_from: check_in,
            date_to: check_out,
            currency: "USD".to_string(),
            daily_rate,
            number_of_persons: (adults + children + infants) as i32,
            total_days,
            sub_total_price,
            discount_value: None,
            tax_value: Some(tax_value),
            fee_breakdown: vec![],
            total_price,
            cancellation_policy: db_core::models::CancellationPolicy::Flexible,
            metadata: BookingMetadata {
                num_adults: adults,
                num_children: children,
                num_infants: infants,
                num_pets: pets,
                message_to_host: None,
                estimated_arrival_time: None,
                is_business_trip: false,
            },
        };

        let booking = db_booking::create_booking(&pool, &new_booking)
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to create booking: {}", e)))?;

        Ok(booking.id)
    }
}

#[server]
pub async fn get_checkout_data(booking_id: Uuid) -> Result<CheckoutDetails, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use db_core::booking as db_booking;
        use db_core::listing as db_listing;
        use web_app_common::api_client::get_pool;

        let pool = get_pool().await;
        let booking = db_booking::get_booking_by_id(&pool, booking_id)
            .await
            .map_err(|e| ServerFnError::new(format!("Booking not found: {}", e)))?;

        let listing_details = db_listing::get_listing_by_id(&pool, booking.listing_id)
            .await
            .map_err(|e| ServerFnError::new(format!("Listing not found: {}", e)))?;
        let listing = listing_details.listing;

        Ok(CheckoutDetails {
            booking: common::models::BookingResponse {
                id: booking.id,
                confirmation_code: booking.confirmation_code,
                guest_id: booking.guest_id,
                listing_id: booking.listing_id,
                status: format!("{:?}", booking.status),
                date_from: booking.date_from,
                date_to: booking.date_to,
                currency: booking.currency,
                daily_rate: booking.daily_rate,
                number_of_persons: booking.number_of_persons,
                total_days: booking.total_days,
                sub_total_price: booking.sub_total_price,
                discount_value: booking.discount_value,
                tax_value: booking.tax_value,
                total_price: booking.total_price,
                cancellation_policy: format!("{:?}", booking.cancellation_policy),
                metadata: common::models::BookingMetadataResponse {
                    num_adults: booking.metadata.num_adults,
                    num_children: booking.metadata.num_children,
                    num_infants: booking.metadata.num_infants,
                    num_pets: booking.metadata.num_pets,
                    message_to_host: booking.metadata.message_to_host.clone(),
                    estimated_arrival_time: booking.metadata.estimated_arrival_time.clone(),
                    is_business_trip: booking.metadata.is_business_trip,
                },
                created_at: booking.created_at,
                updated_at: booking.updated_at,
            },
            listing: common::models::ListingResponse {
                id: listing.id,
                user_id: listing.user_id,
                name: listing.name,
                description: listing.description,
                listing_structure: "Home".to_string(), // Placeholder or fetch
                country: listing.country,
                price_per_night: listing.price_per_night,
                is_active: listing.is_active,
                added_at: listing.added_at,
                owner_name: None,
                primary_image_url: listing.primary_image_url,
                max_guests: listing.max_guests,
                bedrooms: listing.bedrooms,
                full_bathrooms: listing.full_bathrooms,
                latitude: listing.latitude,
                longitude: listing.longitude,
                overall_rating: listing.overall_rating,
                city: listing.city,
                base_currency: listing.base_currency,
                slug: listing.slug,
                listing_details: Some(listing.listing_details.0),
                minimum_stay: listing.minimum_stay,
                days_between_bookings: listing.days_between_bookings,
            },
        })
    }
}

#[server]
pub async fn complete_booking(
    booking_id: Uuid,
    email: String,
    full_name: String,
    _phone: String,
    metadata: common::models::BookingMetadataResponse,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use db_core::booking as db_booking;
        use db_core::listing as db_listing;
        use db_core::models::{BookingStatus, UpdatedBooking, BookingMetadata};
        use web_app_common::api_client::get_pool;
        use web_app_common::email::send_booking_confirmation;

        let pool = get_pool().await;
        tracing::info!("Completing booking for ID: {}", booking_id);

        let update = UpdatedBooking {
            status: Some(BookingStatus::Confirmed),
            metadata: Some(BookingMetadata {
                num_adults: metadata.num_adults,
                num_children: metadata.num_children,
                num_infants: metadata.num_infants,
                num_pets: metadata.num_pets,
                message_to_host: metadata.message_to_host,
                estimated_arrival_time: metadata.estimated_arrival_time,
                is_business_trip: metadata.is_business_trip,
            }),
        };

        let booking = db_booking::update_booking(&pool, booking_id, &update)
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to confirm booking: {}", e)))?;

        let listing_details = db_listing::get_listing_by_id(&pool, booking.listing_id)
            .await
            .map_err(|e| ServerFnError::new(format!("Listing not found: {}", e)))?;

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
        tracing::info!("Booking {} successfully confirmed and email sent.", booking_id);
        leptos_actix::redirect("/");
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

    let checkout_data = Resource::new(
        booking_id,
        |id| async move { get_checkout_data(id).await },
    );

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
        move |(id, email, name, phone, metadata): &(Uuid, String, String, String, common::models::BookingMetadataResponse)| {
            let id = *id;
            let email = email.clone();
            let name = name.clone();
            let phone = phone.clone();
            let metadata = metadata.clone();
            async move { complete_booking(id, email, name, phone, metadata).await }
        },
    );

    view! {
        <div class="container mx-auto px-4 py-12 max-w-6xl">
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

                                            <div class="form-control">
                                                <label class="label"><span class="label-text font-bold">"Message to host (optional)"</span></label>
                                                <textarea 
                                                    class="textarea textarea-bordered h-24" 
                                                    placeholder="Tell the host about your trip..."
                                                    on:input:target=move |ev| message_to_host.set(ev.target().value())
                                                    prop:value=message_to_host
                                                ></textarea>
                                            </div>

                                            <div class="form-control">
                                                <label class="label"><span class="label-text font-bold">"Estimated Arrival Time (optional)"</span></label>
                                                <select class="select select-bordered"
                                                    on:change:target=move |ev| arrival_time.set(ev.target().value())
                                                    prop:value=arrival_time
                                                >
                                                    <option value="">"Select a time"</option>
                                                    <option value="09:00">"09:00 AM – 10:00 AM"</option>
                                                    <option value="12:00">"12:00 PM – 01:00 PM"</option>
                                                    <option value="15:00">"03:00 PM – 04:00 PM"</option>
                                                    <option value="18:00">"06:00 PM – 07:00 PM"</option>
                                                    <option value="21:00">"09:00 PM – 10:00 PM"</option>
                                                </select>
                                            </div>
                                        </div>
                                    </section>

                                    <div class="pt-8">
                                        <button 
                                            class="btn btn-primary btn-lg w-full"
                                            disabled=complete_booking_action.pending()
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
                                                    <span>"$" {listing.price_per_night.unwrap_or_default().to_i64().unwrap_or_default().to_formatted_string(&Locale::en)} " x " {booking.total_days} " nights"</span>
                                                    <span>"$" {booking.sub_total_price.to_i64().unwrap_or_default().to_formatted_string(&Locale::en)}</span>
                                                </div>
                                                <div class="flex justify-between">
                                                    <span class="underline">"Service fee"</span>
                                                    <span>"$" {booking.tax_value.unwrap_or_default().to_i64().unwrap_or_default().to_formatted_string(&Locale::en)}</span>
                                                </div>
                                                <div class="flex justify-between font-bold text-lg pt-4 border-t border-base-200">
                                                    <span>"Total (USD)"</span>
                                                    <span>"$" {booking.total_price.to_i64().unwrap_or_default().to_formatted_string(&Locale::en)}</span>
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
                        <div class="form-control">
                            <label class="label"><span class="label-text">"Email"</span></label>
                            <input type="email" 
                                on:input:target=move |ev| email.set(ev.target().value())
                                value=email class="input input-bordered" readonly />
                        </div>
                        <div class="form-control">
                            <label class="label"><span class="label-text">"Full Name"</span></label>
                            <input type="text"
                                on:input:target=move |ev| name.set(ev.target().value())
                                value=name class="input input-bordered" />
                        </div>
                        <div class="form-control md:col-span-2">
                            <label class="label"><span class="label-text">"Phone Number"</span></label>
                            <input type="tel" 
                                on:input:target=move |ev| phone.set(ev.target().value())
                                value=phone placeholder="+1 (555) 000-0000" class="input input-bordered" />
                        </div>
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
                            <div class="form-control">
                                <label class="label"><span class="label-text">"Email"</span></label>
                                <input type="email" 
                                    on:input:target=move |ev| email.set(ev.target().value())
                                    prop:value=email
                                    placeholder="Your email address" class="input input-bordered" />
                            </div>
                            <div class="form-control">
                                <label class="label"><span class="label-text">"Full Name"</span></label>
                                <input type="text"
                                    on:input:target=move |ev| name.set(ev.target().value())
                                    prop:value=name
                                    placeholder="As it appears on ID" class="input input-bordered" />
                            </div>
                            <div class="form-control md:col-span-2">
                                <label class="label"><span class="label-text">"Phone Number"</span></label>
                                <input type="tel" 
                                    on:input:target=move |ev| phone.set(ev.target().value())
                                    prop:value=phone
                                    placeholder="+1 (555) 000-0000" class="input input-bordered" />
                            </div>
                        </div>
                    </div>
                }.into_any()
            })}
        </Suspense>
    }
}
