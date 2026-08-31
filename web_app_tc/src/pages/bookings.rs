use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};
use web_app_common_tc::api_client::{get_all_bookings, search_listings, ListingSearchParams};

#[page("/bookings")]
pub async fn bookings_page(_cx: &Cx) -> Result {
    let bookings = get_all_bookings(Some(1), Some(50)).await.unwrap_or_default();
    let listings = search_listings(ListingSearchParams {
        per_page: Some(50),
        ..Default::default()
    })
    .await
    .unwrap_or_default();

    view! {
        <div class="max-w-5xl mx-auto px-2 py-8 space-y-8">
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
                    <div class="card bg-base-100 dark:bg-base-200 border border-base-200 dark:border-base-100/20 p-12 text-center rounded-3xl space-y-4 shadow-sm">
                        <div class="text-4xl">"🌴"</div>
                        <h3 class="text-xl font-bold font-serif text-base-content">"No Active Bookings"</h3>
                        <p class="text-xs text-base-content/70 max-w-md mx-auto">
                            "You do not have any active reservations or pending date holds at this time. Explore our luxury villa collection in Jamaica."
                        </p>
                        <div class="pt-2">
                            <a href="/listings" class="btn btn-primary btn-sm rounded-xl font-bold px-6">
                                "Browse Luxury Villas"
                            </a>
                        </div>
                    </div>
                } else {
                    for b in bookings {
                        let booking_id = b.id.to_string();
                        let booking_ref = b.confirmation_code.clone();
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

                        let start_str = b.date_from.format("%b %d, %Y").to_string();
                        let end_str = b.date_to.format("%b %d, %Y").to_string();
                        let total_str = format!("{} {}", b.currency, b.total_price);
                        let is_confirmed = b.status == "confirmed" || b.status == "completed";
                        let is_cancelled = b.status == "cancelled" || b.status == "refunded";
                        let is_hold = b.status == "pending_payment" || b.status == "hold";

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
                                    <a href=(villa_link) class="btn btn-outline btn-sm font-semibold">"View Villa"</a>

                                    if is_confirmed {
                                        <div class="tooltip tooltip-bottom" data-tip=(review_tip)>
                                            <button class="btn btn-sm btn-disabled opacity-50 cursor-not-allowed w-full" disabled=(true)>
                                                "★ Write Review"
                                            </button>
                                        </div>
                                    }

                                    if !is_cancelled {
                                        <button
                                            type="button"
                                            id=(cancel_btn_id)
                                            class="btn btn-outline btn-error btn-sm font-bold mt-1"
                                            onclick=(cancel_onclick)
                                        >
                                            "Cancel Reservation"
                                        </button>
                                    }
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
                    <div class="bg-base-200/60 p-4 rounded-2xl text-xs space-y-1">
                        <div class="font-bold text-base-content">"Cancellation Policy: Flexible"</div>
                        <div class="text-base-content/70">"Full refund available up to 48 hours prior to check-in. Statutory GCT is fully refunded."</div>
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
            var activeCancelBookingId = '';

            function openGuestCancelModal(id, name) {
                try {
                    activeCancelBookingId = id;
                    var nameEl = document.getElementById('cancel-modal-villa-name');
                    if (nameEl) nameEl.innerText = (name ? name : 'Villa Reservation') + ' (#' + id + ')';
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
                        fetch('http://localhost:8081/api/v1/bookings/booking/' + activeCancelBookingId, {
                            method: 'PATCH',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ status: 'refunded' })
                        }).catch(function(err) {
                            console.error('Failed to cancel booking via booking_api:', err);
                        });

                        var badge = document.getElementById('status-badge-' + activeCancelBookingId);
                        if (badge) {
                            badge.className = 'badge badge-error font-bold';
                            badge.innerText = 'Cancelled by Guest';
                        }
                        
                        var cancelBtn = document.getElementById('btn-cancel-' + activeCancelBookingId);
                        if (cancelBtn) {
                            cancelBtn.remove();
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
            "#
        </script>
    }
}
