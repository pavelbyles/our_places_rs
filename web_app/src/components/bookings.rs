use common::models::{BookingResponse, UpdatedBookingRequest};
use leptos::prelude::*;
use leptos_meta::Title;
use uuid::Uuid;
use web_app_common::bookings::{delete_booking_api, get_user_bookings_api, update_booking_api};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BookingFilter {
    All,
    Active,
    Past,
}

#[component]
pub fn MyBookingsPage() -> impl IntoView {
    let auth_context = use_context::<crate::app::AuthContext>().expect("AuthContext missing");
    let user_resource = auth_context.user;

    let filter = RwSignal::new(BookingFilter::All);
    let reload_trigger = RwSignal::new(0);

    // Selected booking for cancellation modal
    let selected_cancel_booking = RwSignal::new(Option::<BookingResponse>::None);
    let cancel_in_progress = RwSignal::new(false);
    let action_message = RwSignal::new(Option::<(bool, String)>::None);
    let review_loading_id = RwSignal::new(Option::<Uuid>::None);

    let handle_start_review = move |booking_id: Uuid| {
        review_loading_id.set(Some(booking_id));
        leptos::task::spawn_local(async move {
            match web_app_common::reviews::get_booking_review_token_server(booking_id).await {
                Ok(eligibility) => {
                    if eligibility.is_eligible {
                        if let Some(tok) = eligibility.token {
                            let navigate = leptos_router::hooks::use_navigate();
                            navigate(&format!("/review/submit/{}", tok), Default::default());
                        }
                    } else {
                        action_message.set(Some((false, eligibility.status_message)));
                        review_loading_id.set(None);
                    }
                }
                Err(e) => {
                    action_message.set(Some((false, format!("Failed to initiate review: {}", e))));
                    review_loading_id.set(None);
                }
            }
        });
    };

    // Fetch bookings when user and reload_trigger change
    let bookings_resource = Resource::new(
        move || (user_resource.get(), reload_trigger.get()),
        |(user_opt, _)| async move {
            match user_opt {
                Some(Ok(Some(u))) => {
                    if let Ok(uid) = uuid::Uuid::parse_str(&u.id) {
                        get_user_bookings_api(uid).await
                    } else {
                        Ok(Vec::new())
                    }
                }
                _ => Ok(Vec::new()),
            }
        },
    );

    let handle_cancel = move || {
        if let Some(booking) = selected_cancel_booking.get() {
            cancel_in_progress.set(true);
            let booking_id = booking.id;
            let is_pending = booking.status.eq_ignore_ascii_case("pending");

            leptos::task::spawn_local(async move {
                let result = if is_pending {
                    delete_booking_api(booking_id).await.map(|_| ())
                } else {
                    update_booking_api(
                        booking_id,
                        UpdatedBookingRequest {
                            status: Some("cancelled".to_string()),
                            metadata: None,
                        },
                    )
                    .await
                    .map(|_| ())
                };

                cancel_in_progress.set(false);
                selected_cancel_booking.set(None);

                match result {
                    Ok(_) => {
                        action_message.set(Some((
                            true,
                            "Your booking has been successfully cancelled.".to_string(),
                        )));
                        reload_trigger.update(|n| *n += 1);
                    }
                    Err(e) => {
                        action_message
                            .set(Some((false, format!("Failed to cancel booking: {}", e))));
                    }
                }
            });
        }
    };

    view! {
        <Title text="My Bookings" />
        <div class="max-w-6xl mx-auto py-10 px-4 sm:px-6 lg:px-8">
            // Header
            <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between pb-6 mb-8 border-b border-base-200 gap-4">
                <div>
                    <h1 class="text-3xl font-extrabold tracking-tight text-base-content">
                        "My Bookings"
                    </h1>
                    <p class="text-sm text-base-content/70 mt-1">
                        "Manage your upcoming getaways, view past reservations, and manage cancellations."
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    <a href="/listings" class="btn btn-primary btn-sm sm:btn-md shadow-sm">
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                        </svg>
                        "Explore Places"
                    </a>
                </div>
            </div>

            // Action feedback banner
            {move || action_message.get().map(|(success, msg)| {
                let alert_class = if success { "alert alert-success shadow-lg mb-6 text-white" } else { "alert alert-error shadow-lg mb-6 text-white" };
                view! {
                    <div class=alert_class>
                        <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                        </svg>
                        <span>{msg}</span>
                        <div>
                            <button class="btn btn-xs btn-ghost" on:click=move |_| action_message.set(None)>"✕"</button>
                        </div>
                    </div>
                }
            })}

            <Suspense fallback=move || view! {
                <div class="flex flex-col items-center justify-center py-20">
                    <span class="loading loading-spinner loading-lg text-primary"></span>
                    <p class="text-sm text-base-content/60 mt-4">"Loading your bookings..."</p>
                </div>
            }>
                {move || match user_resource.get() {
                    Some(Ok(Some(_user))) => {
                        view! {
                            <div>
                                // Filter Tabs
                                {
                                    let all_count = move || {
                                        bookings_resource.get().and_then(|res| res.ok()).map(|b| b.len()).unwrap_or(0)
                                    };
                                    let active_count = move || {
                                        bookings_resource.get().and_then(|res| res.ok()).map(|b| {
                                            b.iter().filter(|x| x.status.eq_ignore_ascii_case("pending") || x.status.eq_ignore_ascii_case("confirmed")).count()
                                        }).unwrap_or(0)
                                    };
                                    let past_count = move || {
                                        bookings_resource.get().and_then(|res| res.ok()).map(|b| {
                                            b.iter().filter(|x| x.status.eq_ignore_ascii_case("completed") || x.status.eq_ignore_ascii_case("cancelled")).count()
                                        }).unwrap_or(0)
                                    };

                                    view! {
                                        <div role="tablist" class="tabs tabs-box bg-base-200/70 p-1.5 rounded-2xl mb-8 flex flex-wrap gap-1 max-w-fit">
                                            <button
                                                type="button"
                                                role="tab"
                                                class="tab font-semibold gap-2 rounded-xl transition-all duration-150 cursor-pointer"
                                                class:tab-active=move || filter.get() == BookingFilter::All
                                                on:click=move |_| filter.set(BookingFilter::All)
                                            >
                                                <span>"All Bookings"</span>
                                                <span class="badge badge-sm" class:badge-primary=move || filter.get() == BookingFilter::All>
                                                    {move || all_count()}
                                                </span>
                                            </button>
                                            <button
                                                type="button"
                                                role="tab"
                                                class="tab font-semibold gap-2 rounded-xl transition-all duration-150 cursor-pointer"
                                                class:tab-active=move || filter.get() == BookingFilter::Active
                                                on:click=move |_| filter.set(BookingFilter::Active)
                                            >
                                                <span>"Upcoming & Active"</span>
                                                <span class="badge badge-sm" class:badge-primary=move || filter.get() == BookingFilter::Active>
                                                    {move || active_count()}
                                                </span>
                                            </button>
                                            <button
                                                type="button"
                                                role="tab"
                                                class="tab font-semibold gap-2 rounded-xl transition-all duration-150 cursor-pointer"
                                                class:tab-active=move || filter.get() == BookingFilter::Past
                                                on:click=move |_| filter.set(BookingFilter::Past)
                                            >
                                                <span>"Past & Cancelled"</span>
                                                <span class="badge badge-sm" class:badge-primary=move || filter.get() == BookingFilter::Past>
                                                    {move || past_count()}
                                                </span>
                                            </button>
                                        </div>
                                    }
                                }

                                // Bookings List
                                {move || match bookings_resource.get() {
                                    Some(Ok(bookings)) => {
                                        let mut filtered_bookings: Vec<BookingResponse> = bookings
                                            .into_iter()
                                            .filter(|b| {
                                                match filter.get() {
                                                    BookingFilter::All => true,
                                                    BookingFilter::Active => {
                                                        b.status.eq_ignore_ascii_case("pending")
                                                            || b.status.eq_ignore_ascii_case("confirmed")
                                                    }
                                                    BookingFilter::Past => {
                                                        b.status.eq_ignore_ascii_case("completed")
                                                            || b.status.eq_ignore_ascii_case("cancelled")
                                                    }
                                                }
                                            })
                                            .collect();

                                        filtered_bookings.sort_by_key(|a| a.date_from);

                                        if filtered_bookings.is_empty() {
                                            view! {
                                                <div class="text-center py-16 px-4 bg-base-200/50 rounded-2xl border border-dashed border-base-300">
                                                    <div class="inline-flex items-center justify-center w-16 h-16 rounded-full bg-primary/10 text-primary mb-4">
                                                        <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4" />
                                                        </svg>
                                                    </div>
                                                    <h3 class="text-lg font-bold text-base-content">"No bookings found"</h3>
                                                    <p class="text-sm text-base-content/60 max-w-sm mx-auto mt-1 mb-6">
                                                        "You don't have any reservations matching this filter. Find your next luxury stay in Jamaica today!"
                                                    </p>
                                                    <a href="/listings" class="btn btn-primary btn-sm">
                                                        "Browse Available Villas"
                                                    </a>
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <div class="grid grid-cols-1 gap-6">
                                                    {filtered_bookings
                                                        .into_iter()
                                                        .map(|booking| {
                                                            view! {
                                                                <BookingItemCard
                                                                    booking=booking
                                                                    on_cancel=move |b| selected_cancel_booking.set(Some(b))
                                                                    on_review=handle_start_review
                                                                    review_loading_id=review_loading_id
                                                                />
                                                            }
                                                        })
                                                        .collect::<Vec<_>>()}
                                                </div>
                                            }.into_any()
                                        }
                                    }
                                    Some(Err(e)) => view! {
                                        <div class="alert alert-error shadow-lg">
                                            <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" />
                                            </svg>
                                            <span>{format!("Error loading bookings: {}", e)}</span>
                                        </div>
                                    }.into_any(),
                                    None => view! { <div class="loading loading-spinner text-primary"></div> }.into_any(),
                                }}
                            </div>
                        }.into_any()
                    }
                    Some(Ok(None)) => view! {
                        <div class="text-center py-16 px-4 bg-base-200/50 rounded-2xl border border-base-300">
                            <div class="inline-flex items-center justify-center w-16 h-16 rounded-full bg-primary/10 text-primary mb-4">
                                <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
                                </svg>
                            </div>
                            <h2 class="text-xl font-bold text-base-content">"Please Log In"</h2>
                            <p class="text-sm text-base-content/60 max-w-sm mx-auto mt-2 mb-6">
                                "You must be signed in to view and manage your reservations."
                            </p>
                            <div class="flex items-center justify-center gap-3">
                                <a href="/login" class="btn btn-primary btn-sm">"Log In"</a>
                                <a href="/register" class="btn btn-outline btn-sm">"Create Account"</a>
                            </div>
                        </div>
                    }.into_any(),
                    Some(Err(_)) => view! {
                        <div class="alert alert-error shadow-lg">
                            <span>"Authentication error. Please try logging in again."</span>
                        </div>
                    }.into_any(),
                    None => view! { <div class="loading loading-spinner text-primary"></div> }.into_any(),
                }}
            </Suspense>

            // Cancellation Confirmation Modal
            {move || selected_cancel_booking.get().map(|booking| {
                let is_pending = booking.status.eq_ignore_ascii_case("pending");
                let title = if is_pending { "Cancel Temporary Hold" } else { "Cancel Reservation" };
                let confirm_label = if is_pending { "Release Hold" } else { "Confirm Cancellation" };

                view! {
                    <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4">
                        <div class="bg-base-100 rounded-2xl max-w-md w-full p-6 shadow-2xl border border-base-200 animate-in fade-in zoom-in-95 duration-150">
                            <div class="flex items-start justify-between pb-3 border-b border-base-200">
                                <h3 class="font-bold text-lg text-base-content flex items-center gap-2">
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 text-error" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                                    </svg>
                                    {title}
                                </h3>
                                <button
                                    class="btn btn-sm btn-circle btn-ghost"
                                    disabled=move || cancel_in_progress.get()
                                    on:click=move |_| selected_cancel_booking.set(None)
                                >
                                    "✕"
                                </button>
                            </div>

                            <div class="py-4 space-y-3">
                                <p class="text-sm text-base-content/80">
                                    "Are you sure you want to cancel booking "
                                    <span class="font-mono font-bold text-primary">{booking.confirmation_code.clone()}</span>
                                    "?"
                                </p>

                                <div class="bg-base-200 rounded-xl p-3 text-xs space-y-1">
                                    <div class="flex justify-between">
                                        <span class="text-base-content/60">"Dates:"</span>
                                        <span class="font-semibold">{format!("{} to {}", booking.date_from, booking.date_to)}</span>
                                    </div>
                                    <div class="flex justify-between">
                                        <span class="text-base-content/60">"Total Amount:"</span>
                                        <span class="font-semibold">{format!("{} {:.2}", booking.currency, booking.total_price)}</span>
                                    </div>
                                    <div class="flex justify-between">
                                        <span class="text-base-content/60">"Cancellation Policy:"</span>
                                        <span class="font-semibold uppercase">{booking.cancellation_policy.clone()}</span>
                                    </div>
                                </div>

                                {(!is_pending).then(|| view! {
                                    <p class="text-xs text-base-content/60 italic">
                                        "Note: Cancellations are subject to the property's cancellation policy terms."
                                    </p>
                                })}
                            </div>

                            <div class="flex items-center justify-end gap-3 pt-3 border-t border-base-200">
                                <button
                                    type="button"
                                    class="btn btn-ghost btn-sm"
                                    disabled=move || cancel_in_progress.get()
                                    on:click=move |_| selected_cancel_booking.set(None)
                                >
                                    "Keep Booking"
                                </button>
                                <button
                                    type="button"
                                    class="btn btn-error btn-sm text-white"
                                    disabled=move || cancel_in_progress.get()
                                    on:click=move |_| handle_cancel()
                                >
                                    {move || if cancel_in_progress.get() {
                                        view! { <span class="loading loading-spinner loading-xs"></span> }.into_any()
                                    } else {
                                        view! { <span>{confirm_label}</span> }.into_any()
                                    }}
                                </button>
                            </div>
                        </div>
                    </div>
                }
            })}
        </div>
    }
}

#[component]
fn BookingItemCard<F, R>(
    booking: BookingResponse,
    on_cancel: F,
    on_review: R,
    review_loading_id: RwSignal<Option<Uuid>>,
) -> impl IntoView
where
    F: Fn(BookingResponse) + Copy + 'static + Send + Sync,
    R: Fn(Uuid) + Copy + 'static + Send + Sync,
{
    let status_lower = booking.status.to_lowercase();
    let is_pending = status_lower == "pending";
    let is_confirmed = status_lower == "confirmed";
    let is_cancelled = status_lower == "cancelled";

    let (badge_class, badge_text) = match status_lower.as_str() {
        "confirmed" => ("badge badge-success text-white font-semibold", "Confirmed"),
        "pending" => (
            "badge badge-warning text-base-content font-semibold",
            "Pending Hold",
        ),
        "completed" => (
            "badge badge-info text-white font-semibold",
            "Completed Stay",
        ),
        "cancelled" => (
            "badge badge-ghost text-base-content/50 font-semibold line-through",
            "Cancelled",
        ),
        _ => ("badge badge-ghost font-semibold", "Unknown"),
    };

    let total_nights = (booking.date_to - booking.date_from).num_days();
    let b_clone = booking.clone();

    // 15-day post-stay review eligibility calculation
    let today = chrono::Utc::now().date_naive();
    let checkout_date = booking.date_to;
    let is_concluded = today >= checkout_date && !is_cancelled;
    let cutoff_date = checkout_date + chrono::Duration::days(15);
    let is_within_15_days = today <= cutoff_date;
    let days_left_to_review = (cutoff_date - today).num_days();
    let is_review_eligible = is_concluded && is_within_15_days;
    let is_review_expired = today > cutoff_date && !is_cancelled;

    view! {
        <div class="card bg-base-100 shadow-md border border-base-200 hover:border-primary/30 transition-all duration-200 overflow-hidden">
            <div class="card-body p-6">
                <div class="flex flex-col lg:flex-row lg:items-center lg:justify-between gap-4 pb-4 border-b border-base-200">
                    // Left: Confirmation Code & Status
                    <div class="flex flex-wrap items-center gap-3">
                        <span class="font-mono text-sm font-bold bg-base-200 px-3 py-1 rounded-lg border border-base-300 text-base-content">
                            {booking.confirmation_code.clone()}
                        </span>
                        <div class=badge_class>{badge_text}</div>
                        <span class="text-xs text-base-content/50">
                            {format!("Booked on {}", booking.created_at.format("%b %d, %Y"))}
                        </span>
                    </div>

                    // Right: Price breakdown
                    <div class="flex items-baseline gap-1 lg:text-right">
                        <span class="text-2xl font-black text-primary">
                            {format!("{} {:.2}", booking.currency, booking.total_price)}
                        </span>
                        <span class="text-xs text-base-content/60">
                            {format!("({} nights total)", total_nights)}
                        </span>
                    </div>
                </div>

                // Details Row
                <div class="grid grid-cols-1 md:grid-cols-3 gap-4 my-4">
                    // Dates
                    <div class="bg-base-200/50 p-3 rounded-xl border border-base-200">
                        <span class="text-xs font-bold text-base-content/60 uppercase tracking-wider block mb-1">
                            "Stay Dates"
                        </span>
                        <div class="font-semibold text-sm text-base-content flex items-center gap-1.5">
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                            </svg>
                            {format!("{} → {}", booking.date_from.format("%b %d, %Y"), booking.date_to.format("%b %d, %Y"))}
                        </div>
                    </div>

                    // Guests
                    <div class="bg-base-200/50 p-3 rounded-xl border border-base-200">
                        <span class="text-xs font-bold text-base-content/60 uppercase tracking-wider block mb-1">
                            "Guests"
                        </span>
                        <div class="font-semibold text-sm text-base-content flex items-center gap-1.5">
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
                            </svg>
                            {format!("{} Adults, {} Children, {} Infants",
                                booking.metadata.num_adults,
                                booking.metadata.num_children,
                                booking.metadata.num_infants
                            )}
                        </div>
                    </div>

                    // Policy & Property Link
                    <div class="bg-base-200/50 p-3 rounded-xl border border-base-200 flex flex-col justify-between">
                        <span class="text-xs font-bold text-base-content/60 uppercase tracking-wider block mb-1">
                            "Cancellation Policy"
                        </span>
                        <div class="font-semibold text-sm text-base-content uppercase">
                            {booking.cancellation_policy.clone()}
                        </div>
                    </div>
                </div>

                // Message or Arrival Note if exists
                {booking.metadata.message_to_host.as_ref().map(|msg| view! {
                    <div class="text-xs bg-base-200/30 p-2.5 rounded-lg border border-base-200 text-base-content/70 italic mb-2">
                        <span class="font-semibold not-italic text-base-content/90 mr-1">"Message to host:"</span>
                        {format!("\"{}\"", msg)}
                    </div>
                })}

                // Action Bar
                <div class="card-actions justify-end items-center gap-3 pt-3 border-t border-base-200">
                    // View Property Button
                    <a
                        href=format!("/listing/{}", booking.listing_id)
                        class="btn btn-sm btn-ghost gap-1.5"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                        </svg>
                        "View Property"
                    </a>

                    // If Pending: Complete Checkout Button
                    {is_pending.then(|| view! {
                        <a
                            href=format!("/checkout/{}", booking.id)
                            class="btn btn-sm btn-primary gap-1.5 shadow-sm"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
                            </svg>
                            "Complete Checkout"
                        </a>
                    })}

                    // If Stay Concluded and Within 15-day Window: Leave a Review
                    {is_review_eligible.then(move || {
                        let b_id = booking.id;
                        let is_this_loading = move || review_loading_id.get() == Some(b_id);

                        let eligibility_resource = Resource::new(
                            move || (),
                            move |_| {
                                let id = b_id;
                                async move {
                                    web_app_common::reviews::get_booking_review_token_server(id).await.ok()
                                }
                            }
                        );

                        view! {
                            <Suspense fallback=move || view! { <div class="btn btn-sm btn-ghost w-32"><span class="loading loading-spinner loading-xs text-primary"></span></div> }>
                                {move || match eligibility_resource.get() {
                                    Some(Some(eligibility)) => {
                                        if eligibility.has_reviewed {
                                            view! {
                                                <span class="text-xs font-semibold text-success flex items-center gap-1.5 py-1.5 px-3 border border-success/30 bg-success/10 rounded-lg">
                                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                                                    </svg>
                                                    "Reviewed"
                                                </span>
                                            }.into_any()
                                        } else if eligibility.is_eligible {
                                            view! {
                                                <button
                                                    type="button"
                                                    class="btn btn-sm btn-primary gap-1.5 shadow-sm"
                                                    disabled=is_this_loading
                                                    on:click=move |_| on_review(b_id)
                                                >
                                                    {move || if is_this_loading() {
                                                        view! { <span class="loading loading-spinner loading-xs"></span> }.into_any()
                                                    } else {
                                                        view! {
                                                            <span class="flex items-center gap-1.5">
                                                                <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-warning fill-warning" viewBox="0 0 24 24" stroke="currentColor">
                                                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
                                                                </svg>
                                                                <span>"Leave a Review"</span>
                                                                <span class="badge badge-warning badge-xs font-bold text-base-content text-[10px] px-1.5 py-0.5">
                                                                    {format!("{}d left", days_left_to_review)}
                                                                </span>
                                                            </span>
                                                        }.into_any()
                                                    }}
                                                </button>
                                            }.into_any()
                                        } else {
                                            view! { <div></div> }.into_any()
                                        }
                                    },
                                    _ => view! { <div></div> }.into_any()
                                }}
                            </Suspense>
                        }
                    })}

                    // If Review Window Expired (> 15 days post-stay)
                    {is_review_expired.then(|| view! {
                        <span class="text-xs text-base-content/50 italic py-1 px-2">
                            "Review window closed"
                        </span>
                    })}

                    // Cancel Booking Option (Available on all active/in-flight bookings: Pending or Confirmed)
                    {(is_pending || is_confirmed).then(move || {
                        let btn_label = if is_pending { "Cancel Hold" } else { "Cancel Booking" };
                        view! {
                            <button
                                type="button"
                                class="btn btn-sm btn-outline btn-error gap-1.5"
                                on:click=move |_| on_cancel(b_clone.clone())
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                                </svg>
                                {btn_label}
                            </button>
                        }
                    })}
                </div>
            </div>
        </div>
    }
}
