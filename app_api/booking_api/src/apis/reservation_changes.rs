//! Quotes and bilateral requests to change a booking's dates or guests.

use super::notifications::{
    dispatch_reservation_change_approved_notification,
    dispatch_reservation_change_declined_notification,
    dispatch_reservation_change_requested_notification,
};
use super::{authorize_booking_party, map_pricing_error, nightly_price};
use crate::settings::TaxSettings;
use actix_web::{HttpRequest, HttpResponse, Responder, web};
use api_core::error::ApiError;
use chrono::{NaiveDate, Utc};
use common::pricing::BookingCalculator;
use db_core::booking as db_booking;
use db_core::listing as db_listing;
use db_core::models::{Booking, BookingStatus};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

async fn compute_reservation_change_quote(
    pool: &PgPool,
    booking: &Booking,
    req_terms: &common::models::ReservationChangeQuoteRequest,
    today_jm: NaiveDate,
) -> Result<common::models::ReservationChangeQuoteResponse, ApiError> {
    if booking.status != BookingStatus::Confirmed {
        return Err(ApiError::BookingNotModifiable(
            "Only confirmed bookings can have their reservation terms modified".to_string(),
        ));
    }

    let new_date_from = req_terms.date_from.unwrap_or(booking.date_from);
    let new_date_to = req_terms.date_to.unwrap_or(booking.date_to);

    let default_adults = if booking.metadata.0.num_adults == 0 && booking.number_of_persons > 0 {
        booking.number_of_persons as u32
    } else {
        booking.metadata.0.num_adults
    };

    let new_adults = req_terms.num_adults.unwrap_or(default_adults);
    let new_children = req_terms
        .num_children
        .unwrap_or(booking.metadata.0.num_children);
    let new_infants = req_terms
        .num_infants
        .unwrap_or(booking.metadata.0.num_infants);
    let new_pets = req_terms.num_pets.unwrap_or(booking.metadata.0.num_pets);

    if new_adults < 1 {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Booking must include at least 1 adult".to_string(),
            ),
        ));
    }

    let new_persons = (new_adults + new_children + new_infants) as i32;

    let dates_changed = new_date_from != booking.date_from || new_date_to != booking.date_to;
    let guests_changed = new_persons != booking.number_of_persons
        || new_adults != default_adults
        || new_children != booking.metadata.0.num_children
        || new_infants != booking.metadata.0.num_infants
        || new_pets != booking.metadata.0.num_pets;

    if !dates_changed && !guests_changed {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "At least one reservation term (dates or guest count) must change".to_string(),
            ),
        ));
    }

    let expires_at_date = common::pricing::validate_reservation_change_dates(
        booking.date_from,
        booking.date_to,
        new_date_from,
        new_date_to,
        today_jm,
    )
    .map_err(|e| match e {
        common::pricing::ReservationChangeValidationError::InvalidDateRange => {
            ApiError::Database(db_core::error::DbError::ValidationError(e.to_string()))
        }
        _ => ApiError::BookingNotModifiable(e.to_string()),
    })?;

    let listing_details = db_listing::get_listing_by_id(pool, booking.listing_id)
        .await
        .map_err(ApiError::Database)?;

    if new_persons > listing_details.listing.max_guests {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(format!(
                "Number of guests ({}) exceeds listing max_guests ({})",
                new_persons, listing_details.listing.max_guests
            )),
        ));
    }

    let payout_ctx = db_core::reservation_change::get_booking_payout_context(
        pool,
        booking.id,
        listing_details.listing.user_id,
        &booking.currency,
    )
    .await
    .map_err(ApiError::Database)?;

    if payout_ctx.is_payout_processed {
        return Err(ApiError::BookingNotModifiable(
            "Reservation cannot be modified after host payout has been processed".to_string(),
        ));
    }

    if dates_changed {
        let available = db_booking::check_availability_excluding(
            pool,
            booking.listing_id,
            new_date_from,
            new_date_to,
            booking.id,
        )
        .await
        .map_err(ApiError::Database)?;

        if !available {
            return Err(ApiError::DateUnavailable);
        }
    }

    let conversion_rate = booking.exchange_rate;
    let base_price = nightly_price(&listing_details.listing)?;
    let base_nightly_rate = if conversion_rate != Decimal::ONE {
        (base_price * conversion_rate).round_dp(2)
    } else {
        base_price
    };

    let raw_overrides = db_listing::get_active_overrides_for_dates(
        pool,
        booking.listing_id,
        new_date_from,
        new_date_to,
    )
    .await
    .map_err(|e| {
        tracing::error!(booking_id = %booking.id, listing_id = %booking.listing_id, error = %e, "failed to load price overrides for change quote");
        ApiError::Database(e)
    })?;

    let converted_overrides: Vec<common::models::PriceOverride> = raw_overrides
        .into_iter()
        .map(|mut ovr| {
            if conversion_rate != Decimal::ONE {
                ovr.nightly_rate = (ovr.nightly_rate * conversion_rate).round_dp(2);
            }
            ovr
        })
        .collect();

    let dynamic_quote = common::pricing::calculate_dynamic_quote(
        base_nightly_rate,
        listing_details.listing.minimum_stay,
        &converted_overrides,
        new_date_from,
        new_date_to,
    )
    .map_err(map_pricing_error)?;

    let tax_rate = if booking.tax_rate > Decimal::ZERO {
        booking.tax_rate
    } else {
        common::reference::SupportedCountry::tax_rate_for(&listing_details.listing.country)
    };

    let clean_fee = if conversion_rate != Decimal::ONE {
        (listing_details.listing.cleaning_fee * conversion_rate).round_dp(2)
    } else {
        listing_details.listing.cleaning_fee
    };

    let dmg_deposit = if conversion_rate != Decimal::ONE {
        (listing_details.listing.damage_deposit * conversion_rate).round_dp(2)
    } else {
        listing_details.listing.damage_deposit
    };

    let extra_guest_fee_per_night = if conversion_rate != Decimal::ONE {
        (listing_details.listing.extra_guest_fee * conversion_rate).round_dp(2)
    } else {
        listing_details.listing.extra_guest_fee
    };

    let days = (new_date_to - new_date_from).num_days() as i32;
    let calculator = BookingCalculator::with_subtotal(
        dynamic_quote.effective_daily_rate,
        days,
        dynamic_quote.subtotal,
    )
    .with_extra_guests(
        new_persons,
        listing_details.listing.guests_included,
        extra_guest_fee_per_night,
    )
    .apply_discounts(
        listing_details.listing.monthly_discount_percentage,
        listing_details.listing.weekly_discount_percentage,
    )
    .with_fees(Some(clean_fee), Some(dmg_deposit))
    .apply_promo(None)
    .apply_taxes(tax_rate)
    .finalize();

    let plat_fee = calculator
        .fee_breakdown
        .iter()
        .find(|f| f.name == "Platform Fee")
        .map(|f| f.amount)
        .unwrap_or(Decimal::ZERO);

    let discounted_subtotal =
        calculator.sub_total_price - calculator.discount_value.unwrap_or(Decimal::ZERO);

    let commission_pct = payout_ctx
        .existing_commission_pct
        .unwrap_or(listing_details.listing.commission_pct);

    let previous_net_payout = payout_ctx.existing_net_payout.unwrap_or_else(|| {
        let prev_gross = common::pricing::host_gross(
            booking.accommodation_net(),
            booking.cleaning_fee,
            payout_ctx.host_exchange_rate,
        );
        common::pricing::calculate_host_payout(prev_gross, commission_pct, Decimal::ZERO).1
    });

    let host_gross_amount = common::pricing::host_gross(
        discounted_subtotal,
        calculator.cleaning_fee,
        payout_ctx.host_exchange_rate,
    );
    let (host_platform_fee_amount, proposed_net_payout) =
        common::pricing::calculate_host_payout(host_gross_amount, commission_pct, Decimal::ZERO);
    let host_payout_delta = proposed_net_payout - previous_net_payout;

    let balance_due = calculator.total_price - booking.total_price;

    let pricing_snapshot = common::models::ReservationChangePricingSnapshot {
        daily_rate: calculator.actual_daily_rate,
        total_days: calculator.total_days,
        extra_guests: calculator.extra_guests,
        extra_guest_fee_per_night: calculator.extra_guest_fee_per_night,
        extra_guest_fee_total: calculator.extra_guest_fee_total,
        sub_total_price: calculator.sub_total_price,
        discount_value: calculator.discount_value,
        discounted_subtotal,
        cleaning_fee: clean_fee,
        damage_deposit: dmg_deposit,
        platform_fee: plat_fee,
        tax_rate,
        tax_value: calculator.tax_value,
        fee_breakdown: calculator.fee_breakdown,
        total_price: calculator.total_price,
        host_gross_amount,
        host_platform_fee_amount,
        host_net_payout_amount: proposed_net_payout,
    };

    Ok(common::models::ReservationChangeQuoteResponse {
        booking_id: booking.id,
        previous_date_from: booking.date_from,
        previous_date_to: booking.date_to,
        proposed_date_from: new_date_from,
        proposed_date_to: new_date_to,
        previous_number_of_persons: booking.number_of_persons,
        proposed_number_of_persons: new_persons,
        proposed_num_adults: new_adults as i32,
        proposed_num_children: new_children as i32,
        proposed_num_infants: new_infants as i32,
        proposed_num_pets: new_pets as i32,
        currency: booking.currency.trim().to_string(),
        previous_total_price: booking.total_price,
        proposed_total_price: calculator.total_price,
        balance_due,
        previous_net_payout,
        proposed_net_payout,
        host_payout_delta,
        pricing_snapshot,
        expires_at_date,
    })
}

#[tracing::instrument(skip(req, pool))]
#[utoipa::path(
    post,
    path = "/api/v1/bookings/{id}/change-quote",
    tag = "bookings",
    request_body = common::models::ReservationChangeQuoteRequest,
    responses(
        (status = 200, description = "Reservation change quote computed", body = common::models::ReservationChangeQuoteResponse),
        (status = 400, description = "Validation error"),
        (status = 404, description = "Booking not found or caller unauthorized"),
        (status = 409, description = "Booking not modifiable or dates unavailable")
    )
)]
pub(super) async fn quote_reservation_change(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    body: web::Json<common::models::ReservationChangeQuoteRequest>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let booking_id = *id;
    let _ = authorize_booking_party(&req, pool.get_ref(), booking_id).await?;

    let booking = db_booking::get_booking_by_id(pool.get_ref(), booking_id)
        .await
        .map_err(ApiError::Database)?;

    let today_jm = (Utc::now() - chrono::Duration::hours(5)).date_naive();
    let quote = compute_reservation_change_quote(pool.get_ref(), &booking, &body, today_jm).await?;

    Ok(HttpResponse::Ok().json(quote))
}

#[tracing::instrument(skip(req, pool))]
#[utoipa::path(
    post,
    path = "/api/v1/bookings/{id}/change-requests",
    tag = "bookings",
    request_body = common::models::CreateReservationChangeRequest,
    responses(
        (status = 201, description = "Reservation change request created", body = common::models::ReservationChangeRequestResponse),
        (status = 400, description = "Validation error"),
        (status = 404, description = "Booking not found or caller unauthorized"),
        (status = 409, description = "Booking not modifiable, dates unavailable, or change request already pending")
    )
)]
pub(super) async fn create_reservation_change_request(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    body: web::Json<common::models::CreateReservationChangeRequest>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let booking_id = *id;
    let (parties, user) = authorize_booking_party(&req, pool.get_ref(), booking_id).await?;

    let initiated_by_role = if user.id == parties.guest_id {
        common::models::ChangeInitiatorRole::Guest
    } else if user.id == parties.host_id {
        common::models::ChangeInitiatorRole::Host
    } else {
        common::models::ChangeInitiatorRole::Admin
    };

    let booking = db_booking::get_booking_by_id(pool.get_ref(), booking_id)
        .await
        .map_err(ApiError::Database)?;

    let today_jm = (Utc::now() - chrono::Duration::hours(5)).date_naive();
    let quote_req = common::models::ReservationChangeQuoteRequest {
        date_from: body.date_from,
        date_to: body.date_to,
        num_adults: body.num_adults,
        num_children: body.num_children,
        num_infants: body.num_infants,
        num_pets: body.num_pets,
    };
    let quote =
        compute_reservation_change_quote(pool.get_ref(), &booking, &quote_req, today_jm).await?;

    let settlement_status = if quote.balance_due > Decimal::ZERO {
        common::models::ChangeSettlementStatus::PendingCharge
    } else if quote.balance_due < Decimal::ZERO {
        common::models::ChangeSettlementStatus::PendingRefund
    } else {
        common::models::ChangeSettlementStatus::NotRequired
    };

    let new_req = db_core::models::NewReservationChangeRequest {
        booking_id: booking.id,
        listing_id: booking.listing_id,
        initiated_by_id: user.id,
        initiated_by_role,
        previous_date_from: quote.previous_date_from,
        previous_date_to: quote.previous_date_to,
        proposed_date_from: quote.proposed_date_from,
        proposed_date_to: quote.proposed_date_to,
        previous_number_of_persons: quote.previous_number_of_persons,
        proposed_number_of_persons: quote.proposed_number_of_persons,
        proposed_num_adults: quote.proposed_num_adults,
        proposed_num_children: quote.proposed_num_children,
        proposed_num_infants: quote.proposed_num_infants,
        proposed_num_pets: quote.proposed_num_pets,
        currency: quote.currency,
        previous_total_price: quote.previous_total_price,
        proposed_total_price: quote.proposed_total_price,
        balance_due: quote.balance_due,
        previous_net_payout: quote.previous_net_payout,
        proposed_net_payout: quote.proposed_net_payout,
        host_payout_delta: quote.host_payout_delta,
        pricing_snapshot: quote.pricing_snapshot,
        settlement_status,
        reason: body.reason.clone(),
        expires_at_date: quote.expires_at_date,
    };

    let created =
        db_core::reservation_change::create_change_request(pool.get_ref(), &new_req, today_jm)
            .await
            .map_err(ApiError::Database)?;

    dispatch_reservation_change_requested_notification(pool.get_ref(), &booking, &created).await;

    let response: common::models::ReservationChangeRequestResponse = created.into();
    Ok(HttpResponse::Created().json(response))
}

#[tracing::instrument(skip(req, pool))]
#[utoipa::path(
    get,
    path = "/api/v1/bookings/{id}/change-requests",
    tag = "bookings",
    responses(
        (status = 200, description = "Reservation change requests for booking", body = Vec<common::models::ReservationChangeRequestResponse>),
        (status = 404, description = "Booking not found or caller unauthorized")
    )
)]
pub(super) async fn list_reservation_change_requests(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let booking_id = *id;
    let _ = authorize_booking_party(&req, pool.get_ref(), booking_id).await?;

    let today_jm = (Utc::now() - chrono::Duration::hours(5)).date_naive();
    let rows = db_core::reservation_change::list_change_requests_for_booking(
        pool.get_ref(),
        booking_id,
        today_jm,
    )
    .await
    .map_err(ApiError::Database)?;

    let response: Vec<common::models::ReservationChangeRequestResponse> =
        rows.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(response))
}

#[tracing::instrument(skip(req, pool, tax_settings, gateway))]
#[utoipa::path(
    post,
    path = "/api/v1/bookings/{id}/change-requests/{request_id}/approve",
    tag = "bookings",
    request_body = common::models::RespondReservationChangeRequest,
    responses(
        (status = 200, description = "Reservation change request approved", body = common::models::ReservationChangeRequestResponse),
        (status = 403, description = "Forbidden - caller is not the required counterparty"),
        (status = 404, description = "Booking or change request not found"),
        (status = 409, description = "Request not pending or dates unavailable"),
        (status = 422, description = "Payment settlement failed")
    )
)]
pub(super) async fn approve_reservation_change_request(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    tax_settings: Option<web::Data<TaxSettings>>,
    gateway: Option<
        web::Data<std::sync::Arc<dyn crate::payment_gateway::PaymentSettlementGateway>>,
    >,
    path: web::Path<(Uuid, Uuid)>,
    body: Option<web::Json<common::models::RespondReservationChangeRequest>>,
) -> Result<impl Responder, ApiError> {
    if let Some(ref b) = body {
        b.validate().map_err(ApiError::ValidationError)?;
    }

    let (booking_id, request_id) = path.into_inner();
    let (parties, user) = authorize_booking_party(&req, pool.get_ref(), booking_id).await?;

    let req_row = db_core::reservation_change::get_change_request_by_id(pool.get_ref(), request_id)
        .await
        .map_err(ApiError::Database)?
        .filter(|r| r.booking_id == booking_id)
        .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
            sqlx::Error::RowNotFound,
        )))?;

    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let is_guest = user.id == parties.guest_id;
    let is_host = user.id == parties.host_id;

    if !is_admin && req_row.initiated_by_id == user.id {
        return Err(ApiError::Forbidden(
            "Initiator cannot self-approve their own reservation change request".to_string(),
        ));
    }

    match req_row.initiated_by_role {
        common::models::ChangeInitiatorRole::Guest => {
            if !is_host && !is_admin {
                return Err(ApiError::Forbidden(
                    "Guest-initiated change requests must be approved by the listing host or an administrator".to_string(),
                ));
            }
        }
        common::models::ChangeInitiatorRole::Host => {
            if !is_guest && !is_admin {
                return Err(ApiError::Forbidden(
                    "Host-initiated change requests must be approved by the booking guest or an administrator".to_string(),
                ));
            }
        }
        common::models::ChangeInitiatorRole::Admin => {
            if !is_guest && !is_host && !is_admin {
                return Err(ApiError::Forbidden(
                    "Not authorized to approve this reservation change request".to_string(),
                ));
            }
        }
    }

    let today_jm = (Utc::now() - chrono::Duration::hours(5)).date_naive();
    if req_row.status == common::models::ReservationChangeStatus::Pending
        && req_row.expires_at_date <= today_jm
    {
        // The request is rejected either way; a failed status flip only delays
        // tidying until the next sweep.
        if let Err(e) = db_core::reservation_change::expire_past_due_change_requests(
            pool.get_ref(),
            Some(booking_id),
            today_jm,
        )
        .await
        {
            tracing::error!(%booking_id, error = ?e, "failed to mark past-due change request expired");
        }
        return Err(ApiError::BookingNotModifiable(
            "Reservation change request has expired".to_string(),
        ));
    }

    if req_row.status != common::models::ReservationChangeStatus::Pending {
        return Err(ApiError::BookingNotModifiable(
            "Only pending reservation change requests can be approved".to_string(),
        ));
    }

    let default_gateway = crate::payment_gateway::SimulatedPaymentGateway;
    let active_gateway: &dyn crate::payment_gateway::PaymentSettlementGateway = gateway
        .as_ref()
        .map(|g| g.get_ref().as_ref())
        .unwrap_or(&default_gateway);

    let (settlement_status, gateway_reference) = match active_gateway
        .settle_balance_delta(
            booking_id,
            request_id,
            req_row.currency.trim(),
            req_row.balance_due,
        )
        .await
    {
        Ok(res) => res,
        Err(err_msg) => {
            let _ =
                db_core::reservation_change::record_failed_settlement(pool.get_ref(), request_id)
                    .await;
            return Err(ApiError::PaymentSettlementFailed(err_msg));
        }
    };

    let supplier = tax_settings
        .as_ref()
        .map(|s| s.to_supplier_info())
        .or_else(|| TaxSettings::from_env().ok().map(|s| s.to_supplier_info()))
        .ok_or_else(|| {
            tracing::error!("Statutory tax supplier settings are missing or invalid");
            ApiError::Internal
        })?;

    let response_note = body.as_ref().and_then(|b| b.response_note.as_deref());
    let (approved_row, updated_booking) = db_core::reservation_change::approve_change_request(
        pool.get_ref(),
        &db_core::reservation_change::ApproveChangeRequestParams {
            id: request_id,
            responder_id: user.id,
            response_note,
            settlement_status,
            gateway_reference: gateway_reference.as_deref(),
            supplier: &supplier,
            today_jm,
        },
    )
    .await
    .map_err(ApiError::Database)?;

    dispatch_reservation_change_approved_notification(
        pool.get_ref(),
        &updated_booking,
        &approved_row,
        &user,
    )
    .await;

    let response: common::models::ReservationChangeRequestResponse = approved_row.into();
    Ok(HttpResponse::Ok().json(response))
}

#[tracing::instrument(skip(req, pool))]
#[utoipa::path(
    post,
    path = "/api/v1/bookings/{id}/change-requests/{request_id}/decline",
    tag = "bookings",
    request_body = common::models::RespondReservationChangeRequest,
    responses(
        (status = 200, description = "Reservation change request declined", body = common::models::ReservationChangeRequestResponse),
        (status = 403, description = "Forbidden - caller is not the required counterparty"),
        (status = 404, description = "Booking or change request not found"),
        (status = 409, description = "Request not pending")
    )
)]
pub(super) async fn decline_reservation_change_request(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
    body: Option<web::Json<common::models::RespondReservationChangeRequest>>,
) -> Result<impl Responder, ApiError> {
    if let Some(ref b) = body {
        b.validate().map_err(ApiError::ValidationError)?;
    }

    let (booking_id, request_id) = path.into_inner();
    let (parties, user) = authorize_booking_party(&req, pool.get_ref(), booking_id).await?;

    let req_row = db_core::reservation_change::get_change_request_by_id(pool.get_ref(), request_id)
        .await
        .map_err(ApiError::Database)?
        .filter(|r| r.booking_id == booking_id)
        .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
            sqlx::Error::RowNotFound,
        )))?;

    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    let is_guest = user.id == parties.guest_id;
    let is_host = user.id == parties.host_id;

    if !is_admin && req_row.initiated_by_id == user.id {
        return Err(ApiError::Forbidden(
            "Initiator cannot decline their own request; use cancel to withdraw it".to_string(),
        ));
    }

    match req_row.initiated_by_role {
        common::models::ChangeInitiatorRole::Guest => {
            if !is_host && !is_admin {
                return Err(ApiError::Forbidden(
                    "Guest-initiated change requests must be declined by the listing host or an administrator".to_string(),
                ));
            }
        }
        common::models::ChangeInitiatorRole::Host => {
            if !is_guest && !is_admin {
                return Err(ApiError::Forbidden(
                    "Host-initiated change requests must be declined by the booking guest or an administrator".to_string(),
                ));
            }
        }
        common::models::ChangeInitiatorRole::Admin => {
            if !is_guest && !is_host && !is_admin {
                return Err(ApiError::Forbidden(
                    "Not authorized to decline this reservation change request".to_string(),
                ));
            }
        }
    }

    let today_jm = (Utc::now() - chrono::Duration::hours(5)).date_naive();
    let response_note = body.as_ref().and_then(|b| b.response_note.as_deref());
    let declined_row = db_core::reservation_change::decline_change_request(
        pool.get_ref(),
        request_id,
        user.id,
        response_note,
        today_jm,
    )
    .await
    .map_err(ApiError::Database)?;

    if let Ok(booking) = db_booking::get_booking_by_id(pool.get_ref(), booking_id).await {
        dispatch_reservation_change_declined_notification(
            pool.get_ref(),
            &booking,
            &declined_row,
            &user,
        )
        .await;
    }

    let response: common::models::ReservationChangeRequestResponse = declined_row.into();
    Ok(HttpResponse::Ok().json(response))
}

#[tracing::instrument(skip(req, pool))]
#[utoipa::path(
    post,
    path = "/api/v1/bookings/{id}/change-requests/{request_id}/cancel",
    tag = "bookings",
    responses(
        (status = 200, description = "Reservation change request cancelled by initiator", body = common::models::ReservationChangeRequestResponse),
        (status = 403, description = "Forbidden - only the initiator or an admin can withdraw a change request"),
        (status = 404, description = "Booking or change request not found"),
        (status = 409, description = "Request not pending")
    )
)]
pub(super) async fn cancel_reservation_change_request(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<impl Responder, ApiError> {
    let (booking_id, request_id) = path.into_inner();
    let (_parties, user) = authorize_booking_party(&req, pool.get_ref(), booking_id).await?;

    let req_row = db_core::reservation_change::get_change_request_by_id(pool.get_ref(), request_id)
        .await
        .map_err(ApiError::Database)?
        .filter(|r| r.booking_id == booking_id)
        .ok_or(ApiError::Database(db_core::error::DbError::Sqlx(
            sqlx::Error::RowNotFound,
        )))?;

    let is_admin = user.roles.contains(&db_core::models::UserRole::Admin);
    if !is_admin && req_row.initiated_by_id != user.id {
        return Err(ApiError::Forbidden(
            "Only the initiator or an administrator can withdraw a pending change request"
                .to_string(),
        ));
    }

    let today_jm = (Utc::now() - chrono::Duration::hours(5)).date_naive();
    let cancelled_row = db_core::reservation_change::cancel_change_request(
        pool.get_ref(),
        request_id,
        user.id,
        is_admin,
        today_jm,
    )
    .await
    .map_err(ApiError::Database)?;

    let response: common::models::ReservationChangeRequestResponse = cancelled_row.into();
    Ok(HttpResponse::Ok().json(response))
}
