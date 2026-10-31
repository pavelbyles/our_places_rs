//! Transactional emails sent when a booking or change request changes state.

use super::{admin_base_url, enqueue_email, guest_base_url};
use chrono::Datelike;
use common::models::{BookingMaterialTerms, is_material_booking_change};
use db_core::listing as db_listing;
use db_core::models::{Booking, BookingStatus};
use rust_decimal::Decimal;
use sqlx::PgPool;

pub(super) async fn dispatch_booking_status_notifications(
    pool: &PgPool,
    old_b: Booking,
    new_b: Booking,
) {
    let pool_clone = pool.clone();

    tokio::spawn(async move {
        let listing = match db_listing::get_listing_by_id(&pool_clone, new_b.listing_id).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(booking_id = %new_b.id, error = ?e, "notification not sent: listing lookup failed");
                return;
            }
        };
        let guest = match db_core::user::get_user_by_id(&pool_clone, new_b.guest_id).await {
            Ok(u) => u,
            Err(e) => {
                tracing::error!(booking_id = %new_b.id, error = ?e, "notification not sent: guest lookup failed");
                return;
            }
        };
        let host = match db_core::user::get_user_by_id(&pool_clone, listing.listing.user_id).await {
            Ok(u) => u,
            Err(e) => {
                tracing::error!(booking_id = %new_b.id, error = ?e, "notification not sent: host lookup failed");
                return;
            }
        };

        // 1. Confirmation: status changed to Confirmed
        if old_b.status != BookingStatus::Confirmed && new_b.status == BookingStatus::Confirmed {
            let doc_number = common::tax_document::document_number(
                common::tax_document::TaxDocumentType::PaymentReceipt,
                new_b.date_from.year(),
                &new_b.confirmation_code,
                1,
            );
            let base_url = guest_base_url();
            let receipt_url = format!(
                "{}/bookings/{}/documents/{}",
                base_url, new_b.id, doc_number
            );

            // Guest confirmation
            let accommodation_net = new_b.accommodation_net();
            let guest_payload = common::email::BookingConfirmationGuestPayload {
                booking_id: new_b.id,
                confirmation_code: new_b.confirmation_code.clone(),
                listing_name: listing.listing.name.clone(),
                date_from: new_b.date_from.to_string(),
                date_to: new_b.date_to.to_string(),
                total_price: new_b.total_price,
                currency: new_b.currency.clone(),
                guest_name: format!("{} {}", guest.first_name, guest.last_name),
                accommodation_net: Some(accommodation_net),
                cleaning_fee: Some(new_b.cleaning_fee),
                tax_rate: Some(new_b.tax_rate),
                tax_amount: new_b.tax_value,
                platform_fee: Some(new_b.platform_fee),
                damage_deposit: Some(new_b.damage_deposit),
                receipt_url: Some(receipt_url),
            };
            enqueue_email(
                &pool_clone,
                &guest.email,
                &format!(
                    "Booking Confirmed - {} (Code: {})",
                    listing.listing.name, new_b.confirmation_code
                ),
                common::email::EmailTemplate::BookingConfirmationGuest.as_str(),
                &guest_payload,
            )
            .await;

            // Host confirmation
            let host_payload = common::email::BookingConfirmationHostPayload {
                booking_id: new_b.id,
                confirmation_code: new_b.confirmation_code.clone(),
                listing_name: listing.listing.name.clone(),
                date_from: new_b.date_from.to_string(),
                date_to: new_b.date_to.to_string(),
                total_payout: new_b.sub_total_price,
                currency: new_b.currency.clone(),
                guest_name: format!("{} {}", guest.first_name, guest.last_name),
                host_name: format!("{} {}", host.first_name, host.last_name),
            };
            enqueue_email(
                &pool_clone,
                &host.email,
                &format!(
                    "New Booking Confirmed - {} (Code: {})",
                    listing.listing.name, new_b.confirmation_code
                ),
                common::email::EmailTemplate::BookingConfirmationHost.as_str(),
                &host_payload,
            )
            .await;
        }
        // 2. Cancellation: status changed to Cancelled
        else if old_b.status != BookingStatus::Cancelled
            && new_b.status == BookingStatus::Cancelled
        {
            let ref_doc = match db_core::tax_document::get_active_by_type(
                &pool_clone,
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
                listing_name: listing.listing.name.clone(),
                date_from: new_b.date_from.to_string(),
                date_to: new_b.date_to.to_string(),
                refund_amount,
                currency: new_b.currency.clone(),
                cancellation_policy: format!("{:?}", new_b.cancellation_policy),
                refund_pct,
                retained_amount,
            };
            enqueue_email(
                &pool_clone,
                &guest.email,
                &format!(
                    "Booking Cancelled - {} (Code: {})",
                    listing.listing.name, new_b.confirmation_code
                ),
                common::email::EmailTemplate::BookingCancelledGuest.as_str(),
                &guest_payload,
            )
            .await;

            let host_payload = common::email::BookingCancelledHostPayload {
                booking_id: new_b.id,
                confirmation_code: new_b.confirmation_code.clone(),
                listing_name: listing.listing.name.clone(),
                date_from: new_b.date_from.to_string(),
                date_to: new_b.date_to.to_string(),
                guest_name: format!("{} {}", guest.first_name, guest.last_name),
                payout_impact: new_b.sub_total_price,
                currency: new_b.currency.clone(),
            };
            enqueue_email(
                &pool_clone,
                &host.email,
                &format!(
                    "Booking Cancelled - {} (Code: {})",
                    listing.listing.name, new_b.confirmation_code
                ),
                common::email::EmailTemplate::BookingCancelledHost.as_str(),
                &host_payload,
            )
            .await;
        }
        // 3. Material Update on Confirmed booking
        else if new_b.status == BookingStatus::Confirmed {
            let old_persons = (old_b.metadata.num_adults
                + old_b.metadata.num_children
                + old_b.metadata.num_infants) as i32;
            let new_persons = (new_b.metadata.num_adults
                + new_b.metadata.num_children
                + new_b.metadata.num_infants) as i32;

            let old_terms = BookingMaterialTerms {
                date_from: old_b.date_from,
                date_to: old_b.date_to,
                number_of_persons: old_persons,
                total_price: old_b.total_price,
            };
            let new_terms = BookingMaterialTerms {
                date_from: new_b.date_from,
                date_to: new_b.date_to,
                number_of_persons: new_persons,
                total_price: new_b.total_price,
            };

            if is_material_booking_change(&old_terms, &new_terms) {
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
                let changes_summary = changes.join(", ");

                let active_receipt = db_core::tax_document::get_active_by_type(
                    &pool_clone,
                    new_b.id,
                    db_core::models::TaxDocumentType::PaymentReceipt,
                )
                .await
                .unwrap_or_else(|e| {
                    tracing::error!(booking_id = %new_b.id, error = ?e, "tax document lookup failed; email sent without document link");
                    None
                });

                let base_url = guest_base_url();
                let (receipt_url, balance_due) = match active_receipt {
                    Some(doc) => (
                        Some(format!(
                            "{}/bookings/{}/documents/{}",
                            base_url, new_b.id, doc.document_number
                        )),
                        doc.balance_due,
                    ),
                    None => (None, None),
                };

                let guest_payload = common::email::BookingUpdatedGuestPayload {
                    booking_id: new_b.id,
                    confirmation_code: new_b.confirmation_code.clone(),
                    listing_name: listing.listing.name.clone(),
                    date_from: new_b.date_from.to_string(),
                    date_to: new_b.date_to.to_string(),
                    total_price: new_b.total_price,
                    currency: new_b.currency.clone(),
                    changes_summary: changes_summary.clone(),
                    receipt_url,
                    balance_due,
                };
                enqueue_email(
                    &pool_clone,
                    &guest.email,
                    &format!(
                        "Booking Updated - {} (Code: {})",
                        listing.listing.name, new_b.confirmation_code
                    ),
                    common::email::EmailTemplate::BookingUpdatedGuest.as_str(),
                    &guest_payload,
                )
                .await;

                let host_payload = common::email::BookingUpdatedHostPayload {
                    booking_id: new_b.id,
                    confirmation_code: new_b.confirmation_code.clone(),
                    listing_name: listing.listing.name.clone(),
                    date_from: new_b.date_from.to_string(),
                    date_to: new_b.date_to.to_string(),
                    total_payout: new_b.sub_total_price,
                    currency: new_b.currency.clone(),
                    guest_name: format!("{} {}", guest.first_name, guest.last_name),
                    changes_summary,
                };
                enqueue_email(
                    &pool_clone,
                    &host.email,
                    &format!(
                        "Booking Updated - {} (Code: {})",
                        listing.listing.name, new_b.confirmation_code
                    ),
                    common::email::EmailTemplate::BookingUpdatedHost.as_str(),
                    &host_payload,
                )
                .await;
            }
        }
        // 4. Completion: status changed to Completed
        else if old_b.status != BookingStatus::Completed
            && new_b.status == BookingStatus::Completed
        {
            let active_invoice = db_core::tax_document::get_active_by_type(
                &pool_clone,
                new_b.id,
                db_core::models::TaxDocumentType::TaxInvoice,
            )
            .await
            .unwrap_or_else(|e| {
                tracing::error!(booking_id = %new_b.id, error = ?e, "tax document lookup failed; email sent without document link");
                None
            });

            let base_url = guest_base_url();
            let (invoice_url, invoice_number) = match active_invoice {
                Some(doc) => (
                    format!(
                        "{}/bookings/{}/documents/{}",
                        base_url, new_b.id, doc.document_number
                    ),
                    doc.document_number,
                ),
                None => {
                    let doc_num = common::tax_document::document_number(
                        common::tax_document::TaxDocumentType::TaxInvoice,
                        new_b.date_to.year(),
                        &new_b.confirmation_code,
                        1,
                    );
                    (
                        format!("{}/bookings/{}/documents/{}", base_url, new_b.id, doc_num),
                        doc_num,
                    )
                }
            };

            let accommodation_net = new_b.accommodation_net();
            let cleaning_fee = new_b.cleaning_fee;
            let tax_amount = new_b.tax_value.unwrap_or_else(|| {
                ((accommodation_net + cleaning_fee) * new_b.tax_rate).round_dp(2)
            });
            let platform_fee = new_b.platform_fee;
            let total_price = accommodation_net + cleaning_fee + tax_amount + platform_fee;

            let guest_payload = common::email::BookingCompletedGuestPayload {
                booking_id: new_b.id,
                confirmation_code: new_b.confirmation_code.clone(),
                listing_name: listing.listing.name.clone(),
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
                &pool_clone,
                &guest.email,
                &format!(
                    "Your Stay Invoice - {} (Code: {})",
                    listing.listing.name, new_b.confirmation_code
                ),
                common::email::EmailTemplate::BookingCompletedGuest.as_str(),
                &guest_payload,
            )
            .await;
        }
    });
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
