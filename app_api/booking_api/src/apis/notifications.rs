//! Transactional emails sent when a booking or change request changes state.

use super::{admin_base_url, enqueue_email, guest_base_url};
use chrono::Datelike;
use common::models::{BookingMaterialTerms, is_material_booking_change};
use db_core::listing as db_listing;
use db_core::models::{Booking, BookingStatus, User};
use rust_decimal::Decimal;
use sqlx::PgPool;

pub(super) async fn dispatch_booking_status_notifications(
    pool: &PgPool,
    old_b: Booking,
    new_b: Booking,
) {
    let pool_clone = pool.clone();

    tokio::spawn(async move {
        let Some(recipients) = Recipients::load(&pool_clone, &new_b).await else {
            return;
        };

        if became(&old_b, &new_b, BookingStatus::Confirmed) {
            send_confirmation_emails(&pool_clone, &recipients, &new_b).await;
        } else if became(&old_b, &new_b, BookingStatus::Cancelled) {
            send_cancellation_emails(&pool_clone, &recipients, &new_b).await;
        } else if new_b.status == BookingStatus::Confirmed {
            send_update_emails(&pool_clone, &recipients, &old_b, &new_b).await;
        } else if became(&old_b, &new_b, BookingStatus::Completed) {
            send_completion_email(&pool_clone, &recipients, &new_b).await;
        }
    });
}

/// Who a booking's status emails go to, and the listing they are about.
struct Recipients {
    listing_name: String,
    guest: User,
    host: User,
}

impl Recipients {
    /// Loads the listing, guest and host; logs and returns `None` if any is missing.
    async fn load(pool: &PgPool, booking: &Booking) -> Option<Self> {
        let listing = db_listing::get_listing_by_id(pool, booking.listing_id)
            .await
            .map_err(|e| {
                tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: listing lookup failed");
            })
            .ok()?;
        let guest = db_core::user::get_user_by_id(pool, booking.guest_id)
            .await
            .map_err(|e| {
                tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: guest lookup failed");
            })
            .ok()?;
        let host = db_core::user::get_user_by_id(pool, listing.listing.user_id)
            .await
            .map_err(|e| {
                tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: host lookup failed");
            })
            .ok()?;
        Some(Self {
            listing_name: listing.listing.name,
            guest,
            host,
        })
    }

    fn guest_name(&self) -> String {
        format!("{} {}", self.guest.first_name, self.guest.last_name)
    }

    fn host_name(&self) -> String {
        format!("{} {}", self.host.first_name, self.host.last_name)
    }

    /// Email subject such as "Booking Confirmed - Villa (Code: ABC123)".
    fn subject(&self, prefix: &str, booking: &Booking) -> String {
        format!(
            "{} - {} (Code: {})",
            prefix, self.listing_name, booking.confirmation_code
        )
    }
}

/// Whether the booking moved into `status` with this change.
fn became(old_b: &Booking, new_b: &Booking, status: BookingStatus) -> bool {
    old_b.status != status && new_b.status == status
}

/// Guest-portal link to one of the booking's tax documents.
fn document_url(booking: &Booking, document_number: &str) -> String {
    format!(
        "{}/bookings/{}/documents/{}",
        guest_base_url(),
        booking.id,
        document_number
    )
}

/// Everyone staying, as counted for material-change checks.
fn party_size(booking: &Booking) -> i32 {
    (booking.metadata.num_adults + booking.metadata.num_children + booking.metadata.num_infants)
        as i32
}

fn material_terms(booking: &Booking) -> BookingMaterialTerms {
    BookingMaterialTerms {
        date_from: booking.date_from,
        date_to: booking.date_to,
        number_of_persons: party_size(booking),
        total_price: booking.total_price,
    }
}

/// Human-readable list of what changed, e.g. "Guest count changed from 2 to 3".
fn changes_summary(old_b: &Booking, new_b: &Booking) -> String {
    let (old_persons, new_persons) = (party_size(old_b), party_size(new_b));
    let mut changes = Vec::new();
    if old_b.date_from != new_b.date_from || old_b.date_to != new_b.date_to {
        changes.push(format!(
            "Dates changed from {} - {} to {} - {}",
            old_b.date_from, old_b.date_to, new_b.date_from, new_b.date_to
        ));
    }
    if old_persons != new_persons {
        changes.push(format!(
            "Guest count changed from {} to {}",
            old_persons, new_persons
        ));
    }
    if old_b.total_price != new_b.total_price {
        changes.push(format!(
            "Total price changed from {} to {}",
            old_b.total_price, new_b.total_price
        ));
    }
    changes.join(", ")
}

/// The booking's active tax document of `doc_type`. A failed lookup is logged
/// and treated as no document, so the email still goes out without the link.
async fn active_document_or_none(
    pool: &PgPool,
    booking: &Booking,
    doc_type: db_core::models::TaxDocumentType,
) -> Option<db_core::models::TaxDocumentRow> {
    db_core::tax_document::get_active_by_type(pool, booking.id, doc_type)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(booking_id = %booking.id, error = ?e, "tax document lookup failed; email sent without document link");
            None
        })
}

async fn send_confirmation_emails(pool: &PgPool, recipients: &Recipients, new_b: &Booking) {
    let doc_number = common::tax_document::document_number(
        common::tax_document::TaxDocumentType::PaymentReceipt,
        new_b.date_from.year(),
        &new_b.confirmation_code,
        1,
    );

    let guest_payload = common::email::BookingConfirmationGuestPayload {
        booking_id: new_b.id,
        confirmation_code: new_b.confirmation_code.clone(),
        listing_name: recipients.listing_name.clone(),
        date_from: new_b.date_from.to_string(),
        date_to: new_b.date_to.to_string(),
        total_price: new_b.total_price,
        currency: new_b.currency.clone(),
        guest_name: recipients.guest_name(),
        accommodation_net: Some(new_b.accommodation_net()),
        cleaning_fee: Some(new_b.cleaning_fee),
        tax_rate: Some(new_b.tax_rate),
        tax_amount: new_b.tax_value,
        platform_fee: Some(new_b.platform_fee),
        damage_deposit: Some(new_b.damage_deposit),
        receipt_url: Some(document_url(new_b, &doc_number)),
    };
    enqueue_email(
        pool,
        &recipients.guest.email,
        &recipients.subject("Booking Confirmed", new_b),
        common::email::EmailTemplate::BookingConfirmationGuest.as_str(),
        &guest_payload,
    )
    .await;

    let host_payload = common::email::BookingConfirmationHostPayload {
        booking_id: new_b.id,
        confirmation_code: new_b.confirmation_code.clone(),
        listing_name: recipients.listing_name.clone(),
        date_from: new_b.date_from.to_string(),
        date_to: new_b.date_to.to_string(),
        total_payout: new_b.sub_total_price,
        currency: new_b.currency.clone(),
        guest_name: recipients.guest_name(),
        host_name: recipients.host_name(),
    };
    enqueue_email(
        pool,
        &recipients.host.email,
        &recipients.subject("New Booking Confirmed", new_b),
        common::email::EmailTemplate::BookingConfirmationHost.as_str(),
        &host_payload,
    )
    .await;
}

async fn send_cancellation_emails(pool: &PgPool, recipients: &Recipients, new_b: &Booking) {
    let ref_doc = match db_core::tax_document::get_active_by_type(
        pool,
        new_b.id,
        db_core::models::TaxDocumentType::RefundReceipt,
    )
    .await
    {
        Ok(doc) => doc,
        Err(e) => {
            // Without the receipt the refund amount is unknown; skip rather than email a wrong figure.
            tracing::error!(booking_id = %new_b.id, error = ?e, "cancellation emails not sent: refund receipt lookup failed");
            return;
        }
    };

    let (refund_amount, refund_pct, retained_amount) = match ref_doc {
        Some(doc) => (doc.total_amount, doc.refund_pct, doc.retained_amount),
        None => (Decimal::ZERO, Some(Decimal::ZERO), Some(Decimal::ZERO)),
    };

    let guest_payload = common::email::BookingCancelledGuestPayload {
        booking_id: new_b.id,
        confirmation_code: new_b.confirmation_code.clone(),
        listing_name: recipients.listing_name.clone(),
        date_from: new_b.date_from.to_string(),
        date_to: new_b.date_to.to_string(),
        refund_amount,
        currency: new_b.currency.clone(),
        cancellation_policy: format!("{:?}", new_b.cancellation_policy),
        refund_pct,
        retained_amount,
    };
    enqueue_email(
        pool,
        &recipients.guest.email,
        &recipients.subject("Booking Cancelled", new_b),
        common::email::EmailTemplate::BookingCancelledGuest.as_str(),
        &guest_payload,
    )
    .await;

    let host_payload = common::email::BookingCancelledHostPayload {
        booking_id: new_b.id,
        confirmation_code: new_b.confirmation_code.clone(),
        listing_name: recipients.listing_name.clone(),
        date_from: new_b.date_from.to_string(),
        date_to: new_b.date_to.to_string(),
        guest_name: recipients.guest_name(),
        payout_impact: new_b.sub_total_price,
        currency: new_b.currency.clone(),
    };
    enqueue_email(
        pool,
        &recipients.host.email,
        &recipients.subject("Booking Cancelled", new_b),
        common::email::EmailTemplate::BookingCancelledHost.as_str(),
        &host_payload,
    )
    .await;
}

/// Emails guest and host when a confirmed booking's dates, party size or
/// price changed; other edits send nothing.
async fn send_update_emails(
    pool: &PgPool,
    recipients: &Recipients,
    old_b: &Booking,
    new_b: &Booking,
) {
    if !is_material_booking_change(&material_terms(old_b), &material_terms(new_b)) {
        return;
    }
    let changes_summary = changes_summary(old_b, new_b);

    let active_receipt = active_document_or_none(
        pool,
        new_b,
        db_core::models::TaxDocumentType::PaymentReceipt,
    )
    .await;
    let (receipt_url, balance_due) = match active_receipt {
        Some(doc) => (
            Some(document_url(new_b, &doc.document_number)),
            doc.balance_due,
        ),
        None => (None, None),
    };

    let guest_payload = common::email::BookingUpdatedGuestPayload {
        booking_id: new_b.id,
        confirmation_code: new_b.confirmation_code.clone(),
        listing_name: recipients.listing_name.clone(),
        date_from: new_b.date_from.to_string(),
        date_to: new_b.date_to.to_string(),
        total_price: new_b.total_price,
        currency: new_b.currency.clone(),
        changes_summary: changes_summary.clone(),
        receipt_url,
        balance_due,
    };
    enqueue_email(
        pool,
        &recipients.guest.email,
        &recipients.subject("Booking Updated", new_b),
        common::email::EmailTemplate::BookingUpdatedGuest.as_str(),
        &guest_payload,
    )
    .await;

    let host_payload = common::email::BookingUpdatedHostPayload {
        booking_id: new_b.id,
        confirmation_code: new_b.confirmation_code.clone(),
        listing_name: recipients.listing_name.clone(),
        date_from: new_b.date_from.to_string(),
        date_to: new_b.date_to.to_string(),
        total_payout: new_b.sub_total_price,
        currency: new_b.currency.clone(),
        guest_name: recipients.guest_name(),
        changes_summary,
    };
    enqueue_email(
        pool,
        &recipients.host.email,
        &recipients.subject("Booking Updated", new_b),
        common::email::EmailTemplate::BookingUpdatedHost.as_str(),
        &host_payload,
    )
    .await;
}

async fn send_completion_email(pool: &PgPool, recipients: &Recipients, new_b: &Booking) {
    let invoice_number =
        active_document_or_none(pool, new_b, db_core::models::TaxDocumentType::TaxInvoice)
            .await
            .map(|doc| doc.document_number)
            .unwrap_or_else(|| {
                common::tax_document::document_number(
                    common::tax_document::TaxDocumentType::TaxInvoice,
                    new_b.date_to.year(),
                    &new_b.confirmation_code,
                    1,
                )
            });
    let invoice_url = document_url(new_b, &invoice_number);

    let accommodation_net = new_b.accommodation_net();
    let cleaning_fee = new_b.cleaning_fee;
    let tax_amount = new_b
        .tax_value
        .unwrap_or_else(|| ((accommodation_net + cleaning_fee) * new_b.tax_rate).round_dp(2));
    let total_price = accommodation_net + cleaning_fee + tax_amount + new_b.platform_fee;

    let guest_payload = common::email::BookingCompletedGuestPayload {
        booking_id: new_b.id,
        confirmation_code: new_b.confirmation_code.clone(),
        listing_name: recipients.listing_name.clone(),
        date_from: new_b.date_from.to_string(),
        date_to: new_b.date_to.to_string(),
        total_price,
        currency: new_b.currency.clone(),
        invoice_url,
        invoice_number,
        accommodation_net,
        cleaning_fee,
        tax_amount,
    };
    enqueue_email(
        pool,
        &recipients.guest.email,
        &recipients.subject("Your Stay Invoice", new_b),
        common::email::EmailTemplate::BookingCompletedGuest.as_str(),
        &guest_payload,
    )
    .await;
}

pub(super) async fn dispatch_reservation_change_requested_notification(
    pool: &PgPool,
    booking: &Booking,
    change_req: &db_core::models::ReservationChangeRequestRow,
) {
    let listing = match db_listing::get_listing_by_id(pool, booking.listing_id).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: listing lookup failed");
            return;
        }
    };
    let guest = match db_core::user::get_user_by_id(pool, booking.guest_id).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: guest lookup failed");
            return;
        }
    };
    let host = match db_core::user::get_user_by_id(pool, listing.listing.user_id).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: host lookup failed");
            return;
        }
    };

    let base_url = guest_base_url();
    let guest_name = format!("{} {}", guest.first_name, guest.last_name);
    let host_name = format!("{} {}", host.first_name, host.last_name);

    let targets: Vec<(&str, String, String, String)> = match change_req.initiated_by_role {
        common::models::ChangeInitiatorRole::Guest => vec![(
            host.email.as_str(),
            host_name,
            guest_name,
            format!("{}/admin/bookings", admin_base_url()),
        )],
        common::models::ChangeInitiatorRole::Host => vec![(
            guest.email.as_str(),
            guest_name,
            host_name,
            format!("{}/bookings", base_url),
        )],
        common::models::ChangeInitiatorRole::Admin => vec![
            (
                guest.email.as_str(),
                guest_name,
                "Our Places Admin".to_string(),
                format!("{}/bookings", base_url),
            ),
            (
                host.email.as_str(),
                host_name,
                "Our Places Admin".to_string(),
                format!("{}/admin/bookings", admin_base_url()),
            ),
        ],
    };

    for (recipient_email, recipient_name, initiator_name, action_url) in targets {
        let payload = common::email::ReservationChangeRequestedPayload {
            booking_id: booking.id,
            change_request_id: change_req.id,
            confirmation_code: booking.confirmation_code.clone(),
            listing_name: listing.listing.name.clone(),
            recipient_name,
            initiator_name,
            initiated_by_role: change_req.initiated_by_role.to_string(),
            previous_date_from: change_req.previous_date_from.to_string(),
            previous_date_to: change_req.previous_date_to.to_string(),
            proposed_date_from: change_req.proposed_date_from.to_string(),
            proposed_date_to: change_req.proposed_date_to.to_string(),
            previous_number_of_persons: change_req.previous_number_of_persons,
            proposed_number_of_persons: change_req.proposed_number_of_persons,
            previous_total_price: change_req.previous_total_price,
            proposed_total_price: change_req.proposed_total_price,
            balance_due: change_req.balance_due,
            currency: change_req.currency.trim().to_string(),
            reason: change_req.reason.clone(),
            action_url,
        };
        enqueue_email(
            pool,
            recipient_email,
            &format!(
                "Reservation Change Requested - {} (Code: {})",
                listing.listing.name, booking.confirmation_code
            ),
            common::email::EmailTemplate::ReservationChangeRequested.as_str(),
            &payload,
        )
        .await;
    }
}

pub(super) async fn dispatch_reservation_change_approved_notification(
    pool: &PgPool,
    booking: &Booking,
    change_req: &db_core::models::ReservationChangeRequestRow,
    responder: &db_core::models::User,
) {
    let listing = match db_listing::get_listing_by_id(pool, booking.listing_id).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: listing lookup failed");
            return;
        }
    };
    let guest = match db_core::user::get_user_by_id(pool, booking.guest_id).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: guest lookup failed");
            return;
        }
    };
    let host = match db_core::user::get_user_by_id(pool, listing.listing.user_id).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: host lookup failed");
            return;
        }
    };

    let active_receipt = db_core::tax_document::get_active_by_type(
        pool,
        booking.id,
        db_core::models::TaxDocumentType::PaymentReceipt,
    )
    .await
    .unwrap_or_else(|e| {
        tracing::error!(booking_id = %booking.id, error = ?e, "tax document lookup failed; email sent without document link");
        None
    });

    let base_url = guest_base_url();
    let receipt_url = active_receipt.map(|doc| {
        format!(
            "{}/bookings/{}/documents/{}",
            base_url, booking.id, doc.document_number
        )
    });

    let responder_name = format!("{} {}", responder.first_name, responder.last_name);
    let recipients = [
        (
            guest.email.as_str(),
            format!("{} {}", guest.first_name, guest.last_name),
        ),
        (
            host.email.as_str(),
            format!("{} {}", host.first_name, host.last_name),
        ),
    ];

    for (recipient_email, recipient_name) in recipients {
        let payload = common::email::ReservationChangeApprovedPayload {
            booking_id: booking.id,
            change_request_id: change_req.id,
            confirmation_code: booking.confirmation_code.clone(),
            listing_name: listing.listing.name.clone(),
            recipient_name,
            responder_name: responder_name.clone(),
            date_from: booking.date_from.to_string(),
            date_to: booking.date_to.to_string(),
            number_of_persons: booking.number_of_persons,
            total_price: booking.total_price,
            balance_due: change_req.balance_due,
            currency: change_req.currency.trim().to_string(),
            settlement_status: change_req.settlement_status.to_string(),
            response_note: change_req.response_note.clone(),
            receipt_url: receipt_url.clone(),
        };
        enqueue_email(
            pool,
            recipient_email,
            &format!(
                "Reservation Change Approved - {} (Code: {})",
                listing.listing.name, booking.confirmation_code
            ),
            common::email::EmailTemplate::ReservationChangeApproved.as_str(),
            &payload,
        )
        .await;
    }
}

pub(super) async fn dispatch_reservation_change_declined_notification(
    pool: &PgPool,
    booking: &Booking,
    change_req: &db_core::models::ReservationChangeRequestRow,
    responder: &db_core::models::User,
) {
    let listing = match db_listing::get_listing_by_id(pool, booking.listing_id).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: listing lookup failed");
            return;
        }
    };
    let initiator = match db_core::user::get_user_by_id(pool, change_req.initiated_by_id).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(booking_id = %booking.id, error = ?e, "notification not sent: initiator lookup failed");
            return;
        }
    };

    let base_url = guest_base_url();
    let booking_url = match change_req.initiated_by_role {
        common::models::ChangeInitiatorRole::Guest => {
            format!("{}/bookings", base_url)
        }
        common::models::ChangeInitiatorRole::Host | common::models::ChangeInitiatorRole::Admin => {
            format!("{}/admin/bookings", admin_base_url())
        }
    };

    let payload = common::email::ReservationChangeDeclinedPayload {
        booking_id: booking.id,
        change_request_id: change_req.id,
        confirmation_code: booking.confirmation_code.clone(),
        listing_name: listing.listing.name.clone(),
        recipient_name: format!("{} {}", initiator.first_name, initiator.last_name),
        responder_name: format!("{} {}", responder.first_name, responder.last_name),
        proposed_date_from: change_req.proposed_date_from.to_string(),
        proposed_date_to: change_req.proposed_date_to.to_string(),
        proposed_number_of_persons: change_req.proposed_number_of_persons,
        response_note: change_req.response_note.clone(),
        booking_url,
    };

    enqueue_email(
        pool,
        &initiator.email,
        &format!(
            "Reservation Change Declined - {} (Code: {})",
            listing.listing.name, booking.confirmation_code
        ),
        common::email::EmailTemplate::ReservationChangeDeclined.as_str(),
        &payload,
    )
    .await;
}
