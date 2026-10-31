//! Booking status changes, deletion and guest transfer.

use super::booking_policy::{BookingRole, authorize_update};
use super::notifications::dispatch_booking_status_notifications;
use super::{authorize_booking_party, is_admin, map_pricing_error, nightly_price, require_admin};
use crate::settings::TaxSettings;
use actix_web::{HttpRequest, HttpResponse, Responder, web};
use api_core::auth::Claims;
use api_core::response::{Payload, respond};
use api_core::settings::Settings;
use api_core::{
    error::ApiError,
    models::{BookingResponse, map_booking_to_response},
};
use chrono::{NaiveDate, Utc};
use common::pricing::BookingCalculator;
use db_core::booking as db_booking;
use db_core::listing as db_listing;
use db_core::models::{BookingMetadata, BookingStatus, FeeItem, UpdatedBooking};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

/// Request payload for updating booking details, status, or door access code.
#[derive(Debug, Default, Deserialize, Validate, ToSchema)]
pub struct UpdatedBookingRequest {
    /// Optional updated booking status.
    pub status: Option<BookingStatus>,
    /// Optional updated booking metadata.
    pub metadata: Option<BookingMetadata>,
    /// Optional physical door lock pin code.
    pub door_access_code: Option<String>,
    /// Optional updated check-in date.
    pub date_from: Option<NaiveDate>,
    /// Optional updated check-out date.
    pub date_to: Option<NaiveDate>,
    /// Optional updated total guest count (Admin override only).
    pub number_of_persons: Option<i32>,
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    patch,
    path = "/api/v1/bookings/booking/{id}",
    tag = "bookings",
    request_body = UpdatedBookingRequest,
    responses(
        (status = 200, description = "Booking updated successfully", body = BookingResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - only hosts or admins may set door access code"),
        (status = 404, description = "Booking not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn update_booking(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    tax_settings: Option<web::Data<TaxSettings>>,
    id: web::Path<Uuid>,
    body: web::Json<UpdatedBookingRequest>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;

    // Only the booking's guest, its host or an admin may touch it (404 otherwise),
    // and each may only change what their role allows (403).
    let (parties, user) = authorize_booking_party(&req, pool.get_ref(), *id).await?;
    let role = if is_admin(&user) {
        BookingRole::Admin
    } else if user.id == parties.host_id {
        BookingRole::Host
    } else {
        BookingRole::Guest
    };
    authorize_update(role, &body).map_err(|reason| {
        tracing::warn!(booking_id = %id, user_id = %user.id, ?role, reason, "booking update refused");
        ApiError::Forbidden(reason.to_string())
    })?;

    let terms_requested =
        body.date_from.is_some() || body.date_to.is_some() || body.number_of_persons.is_some();

    let old_booking = db_booking::get_booking_by_id(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;

    if terms_requested {
        if old_booking.status != BookingStatus::Confirmed {
            return Err(ApiError::BookingNotModifiable(
                "Only confirmed bookings can have their dates modified".to_string(),
            ));
        }

        let jamaica_today = (Utc::now() - chrono::Duration::hours(5)).date_naive();
        let new_check_in = body.date_from.unwrap_or(old_booking.date_from);
        let new_check_out = body.date_to.unwrap_or(old_booking.date_to);

        common::pricing::validate_reservation_change_dates(
            old_booking.date_from,
            old_booking.date_to,
            new_check_in,
            new_check_out,
            jamaica_today,
        )
        .map_err(|e| match e {
            common::pricing::ReservationChangeValidationError::InvalidDateRange => {
                ApiError::Database(db_core::error::DbError::ValidationError(e.to_string()))
            }
            _ => ApiError::BookingNotModifiable(e.to_string()),
        })?;
    }

    let (
        daily_rate,
        total_days,
        sub_total_price,
        discount_value,
        tax_value,
        fee_breakdown,
        total_price,
        cleaning_fee,
        damage_deposit,
        platform_fee,
        tax_rate,
        exchange_rate,
    ) = if terms_requested {
        let new_check_in = body.date_from.unwrap_or(old_booking.date_from);
        let new_check_out = body.date_to.unwrap_or(old_booking.date_to);
        let new_persons = body
            .number_of_persons
            .unwrap_or(old_booking.number_of_persons);

        let listing_details = db_listing::get_listing_by_id(pool.get_ref(), old_booking.listing_id)
            .await
            .map_err(ApiError::Database)?;

        if new_persons < 1 || new_persons > listing_details.listing.max_guests {
            return Err(ApiError::Database(
                db_core::error::DbError::ValidationError(format!(
                    "Number of guests ({}) must be between 1 and listing max_guests ({})",
                    new_persons, listing_details.listing.max_guests
                )),
            ));
        }

        let conversion_rate = old_booking.exchange_rate;
        let base_price = nightly_price(&listing_details.listing)?;
        let base_nightly_rate = if conversion_rate != Decimal::ONE {
            (base_price * conversion_rate).round_dp(2)
        } else {
            base_price
        };

        let raw_overrides = db_listing::get_active_overrides_for_dates(
            pool.get_ref(),
            old_booking.listing_id,
            new_check_in,
            new_check_out,
        )
        .await
        .map_err(|e| {
            tracing::error!(booking_id = %old_booking.id, listing_id = %old_booking.listing_id, error = %e, "failed to load price overrides for repricing");
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
            new_check_in,
            new_check_out,
        )
        .map_err(map_pricing_error)?;

        let tax_rate = if old_booking.tax_rate > Decimal::ZERO {
            old_booking.tax_rate
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

        let extra_guest_fee = if conversion_rate != Decimal::ONE {
            (listing_details.listing.extra_guest_fee * conversion_rate).round_dp(2)
        } else {
            listing_details.listing.extra_guest_fee
        };

        let days = (new_check_out - new_check_in).num_days() as i32;
        let calculator = BookingCalculator::with_subtotal(
            dynamic_quote.effective_daily_rate,
            days,
            dynamic_quote.subtotal,
        )
        .with_extra_guests(
            new_persons,
            listing_details.listing.guests_included,
            extra_guest_fee,
        )
        .apply_discounts(
            listing_details.listing.monthly_discount_percentage,
            listing_details.listing.weekly_discount_percentage,
        )
        .with_fees(Some(clean_fee), Some(dmg_deposit))
        .apply_taxes(tax_rate)
        .finalize();

        let plat_fee = calculator
            .fee_breakdown
            .iter()
            .find(|f| f.name == "Platform Fee")
            .map(|f| f.amount)
            .unwrap_or(Decimal::ZERO);

        let fee_bd: Vec<FeeItem> = calculator
            .fee_breakdown
            .into_iter()
            .map(|f| FeeItem {
                name: f.name,
                amount: f.amount,
            })
            .collect();

        (
            Some(calculator.actual_daily_rate),
            Some(calculator.total_days),
            Some(calculator.sub_total_price),
            calculator.discount_value,
            calculator.tax_value,
            Some(fee_bd),
            Some(calculator.total_price),
            Some(clean_fee),
            Some(dmg_deposit),
            Some(plat_fee),
            Some(tax_rate),
            Some(conversion_rate),
        )
    } else {
        (
            None, None, None, None, None, None, None, None, None, None, None, None,
        )
    };

    let updated_data = UpdatedBooking {
        status: body.status,
        metadata: body.metadata.clone(),
        door_access_code: body.door_access_code.clone(),
        date_from: body.date_from,
        date_to: body.date_to,
        number_of_persons: body.number_of_persons,
        daily_rate,
        total_days,
        sub_total_price,
        discount_value,
        tax_value,
        fee_breakdown,
        total_price,
        cleaning_fee,
        damage_deposit,
        platform_fee,
        tax_rate,
        exchange_rate,
    };

    let supplier = tax_settings
        .as_ref()
        .map(|s| s.to_supplier_info())
        .or_else(|| TaxSettings::from_env().ok().map(|s| s.to_supplier_info()))
        .ok_or_else(|| {
            tracing::error!("Statutory tax supplier settings are missing or invalid");
            ApiError::Internal
        })?;

    let booking = db_booking::update_booking(pool.get_ref(), *id, &updated_data, &supplier)
        .await
        .map_err(ApiError::Database)?;

    dispatch_booking_status_notifications(pool.get_ref(), old_booking, booking.clone()).await;

    Ok(respond(
        &req,
        Payload::Item(map_booking_to_response(booking)),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    delete,
    path = "/api/v1/bookings/booking/{id}",
    tag = "bookings",
    responses(
        (status = 204, description = "Booking deleted"),
        (status = 404, description = "Booking not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn delete_booking(
    claims: Claims,
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    id: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    require_admin(pool.get_ref(), &claims, "delete bookings").await?;
    if !settings.feature_flags.enable_hard_deletes {
        tracing::warn!(booking_id = %id, admin_id = %claims.sub, "hard delete refused: feature flag is off");
        return Err(ApiError::FeatureDisabled("hard_delete".to_string()));
    }
    tracing::info!(booking_id = %id, admin_id = %claims.sub, "hard-deleting booking");
    db_booking::delete_booking(pool.get_ref(), *id)
        .await
        .map_err(ApiError::Database)?;
    Ok(HttpResponse::NoContent().finish())
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    post,
    path = "/api/v1/bookings/bookings/{id}/transfer",
    tag = "bookings",
    request_body = common::models::TransferBookingRequest,
    responses(
        (status = 200, description = "Booking guest transferred", body = BookingResponse),
        (status = 404, description = "Booking not found or not pending"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn transfer_booking(
    req: HttpRequest,
    claims: Claims,
    pool: web::Data<PgPool>,
    id: web::Path<Uuid>,
    body: web::Json<common::models::TransferBookingRequest>,
) -> Result<impl Responder, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    // Moving a booking to another account is admin-only until guests can
    // claim a placeholder booking with proof of ownership.
    require_admin(pool.get_ref(), &claims, "transfer bookings").await?;
    tracing::info!(booking_id = %id, new_guest_id = %body.guest_id, admin_id = %claims.sub, "transferring booking");

    let booking = db_booking::transfer_booking_guest(pool.get_ref(), *id, body.guest_id)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(map_booking_to_response(booking)),
        |_| (),
        actix_web::http::StatusCode::OK,
    ))
}
