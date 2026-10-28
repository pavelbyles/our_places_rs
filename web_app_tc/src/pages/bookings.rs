use chrono::Datelike;
use common::models::{
    BookingResponse, ChangeInitiatorRole, CreateReservationChangeRequest,
    ReservationChangeQuoteRequest, ReservationChangeQuoteResponse,
    ReservationChangeRequestResponse, ReservationChangeStatus, RespondReservationChangeRequest,
    UpdatedBookingRequest,
};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, error::RouterErrorExt, page, path_param, route},
    view::{View, view},
};
use uuid::Uuid;
use web_app_common_tc::{client::ListingSearchParams, get_api_client};

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    exp: usize,
}

fn generate_jwt_for_user(user_id: Uuid) -> String {
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
    let claims = Claims {
        sub: user_id,
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap_or_default()
}

path_param!(id);
path_param!(request_id);

#[route(PATCH "/api/bookings/{id}")]
pub async fn update_booking_api(
    cx: &Cx,
    Json(payload): Json<UpdatedBookingRequest>,
) -> Result<Json<BookingResponse>> {
    let user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_else(|| web_app_common_tc::app_error("Unauthorized"))?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .update_booking_with_auth(id, &payload, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(GET "/api/bookings/{id}/cancellation-quote")]
pub async fn get_cancellation_quote_api(
    cx: &Cx,
) -> Result<Json<common::cancellation::RefundQuote>> {
    let _user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_else(|| web_app_common_tc::app_error("Unauthorized"))?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let api = get_api_client(cx);
    let resp = api
        .get_cancellation_quote(id)
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(POST "/api/bookings/{id}/change-quote")]
pub async fn quote_reservation_change_api(
    cx: &Cx,
    Json(payload): Json<ReservationChangeQuoteRequest>,
) -> Result<Json<ReservationChangeQuoteResponse>> {
    let user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_else(|| web_app_common_tc::app_error("Unauthorized"))?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .quote_reservation_change(id, &payload, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(POST "/api/bookings/{id}/change-requests")]
pub async fn create_reservation_change_request_api(
    cx: &Cx,
    Json(payload): Json<CreateReservationChangeRequest>,
) -> Result<Json<ReservationChangeRequestResponse>> {
    let user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_else(|| web_app_common_tc::app_error("Unauthorized"))?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .create_reservation_change_request(id, &payload, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(GET "/api/bookings/{id}/change-requests")]
pub async fn list_reservation_change_requests_api(
    cx: &Cx,
) -> Result<Json<Vec<ReservationChangeRequestResponse>>> {
    let user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_else(|| web_app_common_tc::app_error("Unauthorized"))?;
    let id_str: &str = path_param::<Id>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .list_reservation_change_requests(id, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(POST "/api/bookings/{id}/change-requests/{request_id}/approve")]
pub async fn approve_reservation_change_request_api(
    cx: &Cx,
) -> Result<Json<ReservationChangeRequestResponse>> {
    let user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_else(|| web_app_common_tc::app_error("Unauthorized"))?;
    let id_str: &str = path_param::<Id>(cx);
    let req_str: &str = path_param::<RequestId>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let req_id =
        Uuid::parse_str(req_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let default_payload = RespondReservationChangeRequest::default();
    let resp = api
        .approve_reservation_change_request(id, req_id, &default_payload, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(POST "/api/bookings/{id}/change-requests/{request_id}/decline")]
pub async fn decline_reservation_change_request_api(
    cx: &Cx,
    Json(payload): Json<RespondReservationChangeRequest>,
) -> Result<Json<ReservationChangeRequestResponse>> {
    let user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_else(|| web_app_common_tc::app_error("Unauthorized"))?;
    let id_str: &str = path_param::<Id>(cx);
    let req_str: &str = path_param::<RequestId>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let req_id =
        Uuid::parse_str(req_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .decline_reservation_change_request(id, req_id, &payload, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[route(POST "/api/bookings/{id}/change-requests/{request_id}/cancel")]
pub async fn cancel_reservation_change_request_api(
    cx: &Cx,
) -> Result<Json<ReservationChangeRequestResponse>> {
    let user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_else(|| web_app_common_tc::app_error("Unauthorized"))?;
    let id_str: &str = path_param::<Id>(cx);
    let req_str: &str = path_param::<RequestId>(cx);
    let id = Uuid::parse_str(id_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let req_id =
        Uuid::parse_str(req_str).map_err(|_| web_app_common_tc::app_error("Invalid UUID"))?;
    let token = generate_jwt_for_user(user.id.unwrap_or_default());
    let api = get_api_client(cx);
    let resp = api
        .cancel_reservation_change_request(id, req_id, Some(&token))
        .await
        .map_err(|e| web_app_common_tc::app_error(format!("Server error: {}", e)))?;
    Ok(Json(resp))
}

#[page("/bookings")]
pub async fn bookings_page(cx: &Cx) -> Result<impl View> {
    let user = web_app_common_tc::auth::get_guest_session(cx)
        .await
        .ok_or_redirect("/login?redirect=%2Fbookings")?;
    let api = get_api_client(cx);
    let bookings = api
        .get_all_bookings(Some(1), Some(50))
        .await
        .unwrap_or_default();
    let listings = api
        .search_listings(ListingSearchParams {
            per_page: Some(50),
            ..Default::default()
        })
        .await
        .unwrap_or_default();

    Ok(view! {
        <div class="max-w-5xl mx-auto px-2 py-8 space-y-8">
            // Booking Success Notification Toast Banner
            <div id="booking-success-toast" class="alert alert-success shadow-lg rounded-2xl hidden flex items-center justify-between transition-all duration-500">
                <div class="flex items-center gap-3">
                    <span class="text-2xl">"🎉"</span>
                    <div>
                        <div class="font-bold text-sm">"Reservation Successfully Confirmed!"</div>
                        <div class="text-xs opacity-90">"Your Jamaican villa stay is officially locked in. Concierge notification dispatched."</div>
                    </div>
                </div>
                <button class="btn btn-ghost btn-xs rounded-full font-bold" onclick="document.getElementById('booking-success-toast').remove()">"✕"</button>
            </div>

            <div class="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4">
                <div>
                    <h1 class="text-3xl font-extrabold tracking-tight">"My Bookings & Stays"</h1>
                    <p class="text-base-content/70 text-sm mt-1">
                        "Manage your upcoming reservations, review completed visits, and handle cancellations."
                    </p>
                </div>
                <a href="/listings" class="btn btn-primary btn-sm rounded-xl font-bold">"+ Book Another Stay"</a>
            </div>

            // Container for Active / Upcoming Reservations
            <div id="upcoming-bookings-container" class="space-y-6">
                if bookings.is_empty() {
                    <div class="card bg-base-100 dark:bg-base-200 border border-base-200 dark:border-base-100/20 p-12 text-center rounded-3xl space-y-4 shadow-sm" id="no-bookings-card">
                        <div class="text-4xl">"🌴"</div>
                        <h3 class="text-xl font-bold font-serif text-base-content">"No Active Bookings"</h3>
                        <p class="text-xs text-base-content/70 max-w-md mx-auto">
                            "You do not have any active reservations or pending date holds at this time. Explore our luxury collection in Jamaica."
                        </p>
                        <div class="pt-2">
                            <a href="/listings" class="btn btn-primary btn-sm rounded-xl font-bold px-6">
                                "Browse Luxury Listings"
                            </a>
                        </div>
                    </div>
                } else {
                    for b in bookings {
                        let booking_id = b.id.to_string();
                        let booking_ref = if !b.confirmation_code.is_empty() {
                            b.confirmation_code.clone()
                        } else {
                            booking_id.clone()
                        };
                        let matched_listing = listings.iter().find(|l| l.id == b.listing_id);
                        let villa_name = matched_listing
                            .map(|l| l.name.clone())
                            .unwrap_or_else(|| "Luxury Jamaican Villa".to_string());
                        let villa_slug = matched_listing
                            .map(|l| l.slug.clone())
                            .unwrap_or_else(|| "listings".to_string());
                        let location = matched_listing
                            .map(|l| {
                                if let Some(city) = &l.city {
                                    format!("{}, {}", city, l.country)
                                } else {
                                    l.country.clone()
                                }
                            })
                            .unwrap_or_else(|| "Jamaica".to_string());
                        let image_url = matched_listing
                            .and_then(|l| l.primary_image_url.clone())
                            .unwrap_or_else(|| "https://images.unsplash.com/photo-1580587771525-78b9dba3b914?auto=format&fit=crop&w=600&q=80".to_string());

                        let token = generate_jwt_for_user(user.id.unwrap_or_default());
                        let url = format!("{}/api/v1/bookings/{}/messages", common::app_client::booking_api_url(), b.id);
                        let client = reqwest::Client::new();
                        let msgs_wrapper = if let Ok(res) = client.get(&url).header("Authorization", format!("Bearer {}", token)).send().await {
                            if res.status().is_success() {
                                res.json::<common::models::BookingMessagesWrapper>().await.ok()
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let unread_count = msgs_wrapper.as_ref().map(|w| w.unread_count).unwrap_or(0);
                        let pending_change_req = if b.status.eq_ignore_ascii_case("confirmed") {
                            api.list_reservation_change_requests(b.id, Some(&token))
                                .await
                                .unwrap_or_default()
                                .into_iter()
                                .find(|r| r.status == ReservationChangeStatus::Pending)
                        } else {
                            None
                        };

                        let start_str = b.date_from.format("%b %d, %Y").to_string();
                        let end_str = b.date_to.format("%b %d, %Y").to_string();
                        let total_str = format!("{} {}", b.currency, b.total_price);
                        let is_confirmed = b.status.eq_ignore_ascii_case("confirmed") || b.status.eq_ignore_ascii_case("completed");
                        let is_cancelled = b.status.eq_ignore_ascii_case("cancelled") || b.status.eq_ignore_ascii_case("refunded");
                        let is_hold = b.status.eq_ignore_ascii_case("pending") || b.status.eq_ignore_ascii_case("pending_payment") || b.status.eq_ignore_ascii_case("hold");

                        let badge_class = if is_confirmed {
                            "badge badge-success font-bold"
                        } else if is_hold {
                            "badge badge-warning font-bold"
                        } else if is_cancelled {
                            "badge badge-error font-bold"
                        } else {
                            "badge badge-neutral font-semibold"
                        };

                        let status_label = if is_confirmed {
                            "Confirmed Stay"
                        } else if is_hold {
                            "Pending Hold"
                        } else if is_cancelled {
                            "Cancelled"
                        } else {
                            &b.status
                        };

                        let card_id = format!("booking-card-{}", booking_id);
                        let status_badge_id = format!("status-badge-{}", booking_id);
                        let booking_actions_id = format!("booking-actions-{}", booking_id);
                        let cancel_btn_id = format!("btn-cancel-{}", booking_id);
                        let cancel_onclick = format!("openGuestCancelModal('{}', '{}')", booking_id, villa_name.replace('\'', "\\'"));
                        let villa_link = format!("/listings/{}", villa_slug);
                        let review_tip = format!("Reviews unlock after checkout on {}", end_str);
                        let formatted_ref = format!("#{}", booking_ref);
                        let formatted_total = format!("Total: {}", total_str);
                        let booked_on_str = b.created_at.format("Booked on %b %d, %Y").to_string();
                        let checkin_str = format!("{} (3:00 PM)", start_str);
                        let checkout_str = format!("{} (11:00 AM)", end_str);

                        <div class="card bg-base-100 border border-base-300 shadow-md p-6 rounded-3xl space-y-6" id=(card_id)>
                            <div class="flex flex-wrap justify-between items-center gap-2 border-b border-base-200 pb-4">
                                <div class="flex items-center gap-3">
                                    <span class=(badge_class) id=(status_badge_id)>(status_label)</span>
                                    <span class="text-xs text-base-content/60 font-mono">(formatted_ref)</span>
                                </div>
                                <div class="text-xs text-base-content/70 flex items-center gap-2">
                                    <span>(formatted_total)</span>
                                    <span>"·"</span>
                                    <span>(booked_on_str)</span>
                                </div>
                            </div>

                            if let Some(ref req) = pending_change_req {
                                let req_id_str = req.id.to_string();
                                let prop_dates = format!(
                                    "{} – {} ({} nights, {} guests)",
                                    req.proposed_date_from.format("%b %d, %Y"),
                                    req.proposed_date_to.format("%b %d, %Y"),
                                    (req.proposed_date_to - req.proposed_date_from).num_days(),
                                    req.proposed_number_of_persons
                                );
                                let bal_label = if req.balance_due > rust_decimal::Decimal::ZERO {
                                    format!("Extra Charge: {} {:.2}", req.currency, req.balance_due)
                                } else if req.balance_due < rust_decimal::Decimal::ZERO {
                                    format!("Refund: {} {:.2}", req.currency, req.balance_due.abs())
                                } else {
                                    "No Price Difference".to_string()
                                };
                                let is_host_proposed = req.initiated_by_role == ChangeInitiatorRole::Host
                                    || req.initiated_by_role == ChangeInitiatorRole::Admin;

                                <div
                                    id=(format!("pending-change-banner-{}", booking_id))
                                    class="bg-warning/10 border border-warning/30 rounded-2xl p-4 text-xs space-y-2"
                                >
                                    <div class="flex flex-wrap items-center justify-between gap-2">
                                        <div class="font-bold text-sm text-base-content flex items-center gap-1.5">
                                            <span>(if is_host_proposed { "⚡ Host Proposed Reservation Change" } else { "⏳ Change Request Pending Host Approval" })</span>
                                        </div>
                                        <span class="badge badge-warning badge-sm font-bold">(bal_label)</span>
                                    </div>
                                    <div class="text-base-content/80">
                                        <strong>"Proposed Stay: "</strong>
                                        <span>(prop_dates)</span>
                                        " · "
                                        <strong>"New Total: "</strong>
                                        <span>(format!("{} {:.2}", req.currency, req.proposed_total_price))</span>
                                    </div>
                                    if let Some(ref reason) = req.reason {
                                        <div class="text-base-content/70 italic">
                                            (format!("Note: \"{}\"", reason))
                                        </div>
                                    }
                                    <div class="flex items-center gap-2 pt-1">
                                        if is_host_proposed {
                                            <button
                                                type="button"
                                                id=(format!("btn-approve-change-{}", req_id_str))
                                                class="btn btn-success btn-xs text-white font-bold"
                                                onclick=(format!("respondToGuestChangeRequest('{}', '{}', 'approve')", booking_id, req_id_str))
                                            >
                                                "✓ Approve Change"
                                            </button>
                                            <button
                                                type="button"
                                                id=(format!("btn-decline-change-{}", req_id_str))
                                                class="btn btn-outline btn-error btn-xs font-bold"
                                                onclick=(format!("respondToGuestChangeRequest('{}', '{}', 'decline')", booking_id, req_id_str))
                                            >
                                                "✕ Decline"
                                            </button>
                                        } else {
                                            <button
                                                type="button"
                                                id=(format!("btn-cancel-change-{}", req_id_str))
                                                class="btn btn-outline btn-xs font-bold"
                                                onclick=(format!("respondToGuestChangeRequest('{}', '{}', 'cancel')", booking_id, req_id_str))
                                            >
                                                "Withdraw Request"
                                            </button>
                                        }
                                    </div>
                                </div>
                            }

                            <div class="grid grid-cols-1 md:grid-cols-4 gap-6 items-center">
                                <img
                                    src=(image_url)
                                    alt=(villa_name.clone())
                                    class="w-full h-36 rounded-2xl object-cover"
                                />

                                <div class="md:col-span-2 space-y-2">
                                    <h3 class="text-xl font-bold">(villa_name.clone())</h3>
                                    <p class="text-xs text-base-content/70">(location)</p>
                                    <div class="grid grid-cols-2 gap-2 text-xs pt-2">
                                        <div>
                                            <span class="text-base-content/50 font-bold block">"Check-in:"</span>
                                            <span class="font-semibold text-base-content">(checkin_str)</span>
                                        </div>
                                        <div>
                                            <span class="text-base-content/50 font-bold block">"Check-out:"</span>
                                            <span class="font-semibold text-base-content">(checkout_str)</span>
                                        </div>
                                    </div>
                                </div>

                                <div class="flex flex-col gap-2" id=(booking_actions_id)>
                                    <a href=(villa_link) class="btn btn-outline btn-sm font-semibold">"View Listing"</a>

                                    if is_confirmed {
                                        <div class="tooltip tooltip-bottom" data-tip=(review_tip)>
                                            <button class="btn btn-sm btn-disabled opacity-50 cursor-not-allowed w-full" disabled=(true)>
                                                "★ Write Review"
                                            </button>
                                        </div>
                                    }

                                    if is_cancelled {
                                        <button
                                            type="button"
                                            class="btn btn-sm btn-disabled opacity-50 cursor-not-allowed w-full mt-1"
                                            disabled=(true)
                                        >
                                            "Cancelled"
                                        </button>
                                    } else {
                                        <button
                                            type="button"
                                            id=(cancel_btn_id)
                                            class="btn btn-outline btn-error btn-sm font-bold mt-1"
                                            onclick=(cancel_onclick)
                                        >
                                            "Cancel Reservation"
                                        </button>
                                    }

                                    if b.status.eq_ignore_ascii_case("confirmed") {
                                        let change_dates_url = format!("/bookings/{}/change-dates", booking_id);
                                        <a
                                            href=(change_dates_url)
                                            class="btn btn-outline btn-warning btn-sm font-bold mt-1"
                                            id=(format!("btn-change-dates-{}", booking_id))
                                        >
                                            "📅 Modify Stay"
                                        </a>
                                    }

                                    if b.status.eq_ignore_ascii_case("confirmed") || b.status.eq_ignore_ascii_case("completed") {
                                        let year = b.date_from.year();
                                        let doc_num = common::tax_document::document_number(
                                            common::tax_document::TaxDocumentType::PaymentReceipt,
                                            year,
                                            &b.confirmation_code,
                                            1,
                                        );
                                        let doc_url = format!("/bookings/{}/documents/{}", b.id, doc_num);
                                        <a
                                            href=(doc_url)
                                            class="btn btn-outline btn-sm font-bold mt-1"
                                            id=(format!("btn-receipt-{}", booking_id))
                                        >
                                            "📄 Tax Receipt"
                                        </a>
                                    }

                                    <a
                                        href=(format!("/bookings/{}/messages", booking_ref))
                                        class="btn btn-outline btn-primary btn-sm font-bold mt-1 relative"
                                    >
                                        "💬 Message Host"
                                        if unread_count > 0 {
                                            <span class="badge badge-error badge-sm absolute -top-2 -right-2 font-bold">(unread_count)</span>
                                        }
                                    </a>
                                </div>
                            </div>
                        </div>
                    }
                }
            </div>

            // Cancellation Confirmation Modal
            <dialog id="cancel-booking-modal" class="modal modal-bottom sm:modal-middle">
                <div class="modal-box rounded-3xl p-6 space-y-4">
                    <div class="flex items-center gap-3 text-error">
                        <span class="text-2xl">"⚠️"</span>
                        <h3 class="font-extrabold text-lg text-base-content">"Cancel Reservation?"</h3>
                    </div>
                    <p class="text-sm text-base-content/80">
                        "Are you sure you want to cancel your reservation for "
                        <strong class="font-bold" id="cancel-modal-villa-name">"this stay"</strong>
                        "?"
                    </p>
                    <div id="cancel-modal-quote-details" class="bg-base-200/60 p-4 rounded-2xl text-xs space-y-1">
                        <div class="font-bold text-base-content">"Cancellation Refund Preview"</div>
                        <div class="text-base-content/70">"Calculating estimated refund according to statutory cancellation policy..."</div>
                    </div>
                    <div class="modal-action flex justify-end gap-2 pt-2">
                        <form method="dialog">
                            <button class="btn btn-ghost btn-sm font-semibold">"Keep Reservation"</button>
                        </form>
                        <button
                            type="button"
                            class="btn btn-error btn-sm font-bold text-error-content"
                            onclick="confirmCancellation()"
                        >
                            "Yes, Cancel Booking"
                        </button>
                    </div>
                </div>
            </dialog>
        </div>

        <script>
            r#"
            var activeCancelBookingId = null;

            function respondToGuestChangeRequest(bookingId, requestId, action) {
                try {
                    var url = '/api/bookings/' + bookingId + '/change-requests/' + requestId + '/' + action;
                    var opts = {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json',
                            'Accept': 'application/json'
                        }
                    };
                    if (action === 'decline') {
                        opts.body = JSON.stringify({ reason: 'Declined by guest' });
                    }
                    fetch(url, opts)
                        .then(function(res) {
                            if (!res.ok) {
                                return res.text().then(function(t) { throw new Error(t || ('Status ' + res.status)); });
                            }
                            return res.json();
                        })
                        .then(function() {
                            window.location.reload();
                        })
                        .catch(function(err) {
                            console.error('Failed to process change request action:', err);
                            alert('Failed to process change request: ' + err.message);
                        });
                } catch(e) {
                    console.error('Change request action error:', e);
                }
            }

            function openGuestCancelModal(bookingId, villaName) {
                try {
                    activeCancelBookingId = bookingId;
                    var nameEl = document.getElementById('cancel-modal-villa-name');
                    if (nameEl) nameEl.innerText = villaName || 'your stay';
                    var quoteEl = document.getElementById('cancel-modal-quote-details');
                    if (quoteEl) {
                        quoteEl.innerHTML = '<div class="flex items-center gap-2 text-base-content/70"><span class="loading loading-spinner loading-xs"></span> Calculating refund quote...</div>';
                        fetch('/api/bookings/' + bookingId + '/cancellation-quote')
                            .then(function(r) { return r.json(); })
                            .then(function(data) {
                                if (data && data.refund_total !== undefined) {
                                    var pct = Math.round(parseFloat(data.policy_pct) * 100);
                                    quoteEl.innerHTML = 
                                        '<div class="flex justify-between items-center py-0.5"><span class="font-bold text-base-content">Refund Schedule:</span><span class="badge badge-sm badge-success font-bold">' + pct + '% accommodation</span></div>' +
                                        '<div class="flex justify-between items-center py-0.5"><span class="font-semibold text-base-content">Refund Total:</span><span class="text-success font-extrabold text-sm">$' + data.refund_total + '</span></div>' +
                                        '<div class="flex justify-between items-center py-0.5 text-base-content/70"><span>Retained / Fees:</span><span>$' + data.retained_total + '</span></div>' +
                                        '<div class="text-[10px] text-base-content/60 pt-1 border-t border-base-200 mt-1">Includes pro-rated GCT refund. Statutory refund receipt (REF) will be issued.</div>';
                                } else {
                                    quoteEl.innerHTML = '<div class="text-xs text-base-content/70">Refund will be processed according to property cancellation schedule.</div>';
                                }
                            })
                            .catch(function(err) {
                                quoteEl.innerHTML = '<div class="text-xs text-base-content/70">Refund will be processed according to property cancellation schedule.</div>';
                            });
                    }
                    var modal = document.getElementById('cancel-booking-modal');
                    if (modal) modal.showModal();
                } catch(e) {
                    console.error('Failed to open modal:', e);
                }
            }

            function confirmCancellation() {
                try {
                    var modal = document.getElementById('cancel-booking-modal');
                    if (modal) modal.close();
                    
                    if (activeCancelBookingId) {
                        fetch('/api/bookings/' + activeCancelBookingId, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ status: 'refunded' })
                        }).catch(function(err) {
                            console.error('Failed to cancel booking via server-side route:', err);
                        });

                        var badge = document.getElementById('status-badge-' + activeCancelBookingId);
                        if (badge) {
                            badge.className = 'badge badge-error font-bold';
                            badge.innerText = 'Cancelled';
                        }
                        
                        var cancelBtn = document.getElementById('btn-cancel-' + activeCancelBookingId);
                        if (cancelBtn) {
                            cancelBtn.disabled = true;
                            cancelBtn.className = 'btn btn-sm btn-disabled opacity-50 cursor-not-allowed w-full mt-1';
                            cancelBtn.innerText = 'Cancelled';
                            cancelBtn.removeAttribute('onclick');
                        }
                        
                        var card = document.getElementById('booking-card-' + activeCancelBookingId);
                        if (card) {
                            card.style.opacity = '0.75';
                            var alertDiv = document.createElement('div');
                            alertDiv.className = 'alert alert-info text-xs font-semibold rounded-2xl mb-4';
                            alertDiv.innerText = 'Booking #' + activeCancelBookingId + ' has been successfully cancelled. Your refund has been initiated.';
                            card.prepend(alertDiv);
                        }
                    }
                } catch(e) {
                    console.error('Cancellation error:', e);
                }
            }
            
            (function() {
                try {
                    // Clean up any legacy localStorage mock bookings
                    localStorage.removeItem('op_guest_bookings');

                    if (window.location.search.indexOf('new_booking=true') !== -1) {
                        var toast = document.getElementById('booking-success-toast');
                        if (toast) {
                            toast.classList.remove('hidden');
                        }
                    }
                } catch(err) {
                    console.error('Bookings script error:', err);
                }
            })();
            "#
        </script>
    })
}
