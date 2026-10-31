//! Internal cron endpoints: notification sweeps, stay completion and hold expiry.

use super::notifications::dispatch_booking_status_notifications;
use super::{enqueue_email, guest_base_url};
use crate::settings::{CronSettings, TaxSettings};
use actix_web::{HttpRequest, HttpResponse, web};
use api_core::error::ApiError;
use chrono::{NaiveDate, Utc};
use db_core::booking as db_booking;
use db_core::models::BookingStatus;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use subtle::ConstantTimeEq;
use utoipa::ToSchema;
use uuid::Uuid;

/// Summary response returned by the scheduled notification sweep cron job.
#[derive(Debug, Serialize, ToSchema)]
pub struct CronSweepResponse {
    /// Status description of the sweep execution.
    pub status: String,
    /// Number of pre-arrival guide notifications dispatched.
    pub pre_arrival_processed: usize,
    /// Number of payment hold expiry reminders dispatched.
    pub hold_reminders_processed: usize,
}

fn verify_cron_secret(
    req: &HttpRequest,
    cron_settings: Option<&web::Data<CronSettings>>,
) -> Result<(), ApiError> {
    let configured_secret = cron_settings
        .map(|s| s.cron_secret.clone())
        .or_else(|| CronSettings::from_env().ok().map(|s| s.cron_secret))
        .ok_or_else(|| {
            tracing::error!("Cron settings are not configured");
            ApiError::Internal
        })?;

    let header_val = req.headers().get("x-cron-secret");
    let has_header = subtle::Choice::from(header_val.is_some() as u8);
    let provided_bytes = header_val.map(|v| v.as_bytes()).unwrap_or(b"");

    let configured_hash = Sha256::digest(configured_secret.as_bytes());
    let provided_hash = Sha256::digest(provided_bytes);

    let is_valid = bool::from(has_header & configured_hash.ct_eq(&provided_hash));

    if !is_valid {
        tracing::warn!("Unauthorized attempt to trigger cron endpoint");
        return Err(ApiError::Unauthorized(
            "Invalid or missing cron secret".into(),
        ));
    }

    Ok(())
}

#[tracing::instrument(skip(req, pool, cron_settings))]
#[utoipa::path(
    post,
    path = "/api/v1/internal/cron/process-scheduled-notifications",
    tag = "bookings",
    responses(
        (status = 200, description = "Scheduled notification sweep completed", body = CronSweepResponse),
        (status = 401, description = "Unauthorized - invalid or missing cron secret"),
        (status = 500, description = "Internal server error")
    )
)]
/// Scheduled cron task endpoint that sweeps pending arrivals and upcoming bookings for automated transactional notifications.
pub async fn process_scheduled_notifications(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    cron_settings: Option<web::Data<CronSettings>>,
) -> Result<HttpResponse, ApiError> {
    verify_cron_secret(&req, cron_settings.as_ref())?;

    run_notification_sweep(pool.get_ref())
        .await
        .map(|sweep| HttpResponse::Ok().json(sweep))
}

/// Sends the due pre-arrival guides, host arrival reminders and expiring-hold
/// warnings. Each notification is claimed in `notification_log` first, so
/// repeated runs never send one twice.
async fn run_notification_sweep(pool: &PgPool) -> Result<CronSweepResponse, ApiError> {
    let mut pre_arrival_count = 0;
    let mut hold_reminders_count = 0;

    // Sweep 48h Pre-Arrival Candidates
    let pre_arrival_candidates = db_core::notification_log::get_48h_pre_arrival_candidates(pool)
        .await
        .map_err(ApiError::Database)?;

    for candidate in pre_arrival_candidates {
        let claimed_guest = db_core::notification_log::claim_and_log_notification(
            pool,
            candidate.booking_id,
            "pre_arrival_guide_guest",
            candidate.guest_id,
        )
        .await
        .map_err(ApiError::Database)?;

        if claimed_guest {
            let wifi_ssid = candidate
                .listing_details
                .0
                .get("wifi_ssid")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let wifi_password = candidate
                .listing_details
                .0
                .get("wifi_password")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let check_in_instructions = candidate
                .listing_details
                .0
                .get("check_in_instructions")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let address = candidate
                .listing_details
                .0
                .get("address")
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
                .unwrap_or_else(|| {
                    candidate
                        .listing_city
                        .clone()
                        .unwrap_or_else(|| "Jamaica".to_string())
                });
            let door_code = candidate
                .door_access_code
                .clone()
                .unwrap_or_else(|| "Pending Host Setup".to_string());

            let guest_payload = common::email::PreArrivalGuideGuestPayload {
                booking_id: candidate.booking_id,
                confirmation_code: candidate.confirmation_code.clone(),
                listing_name: candidate.listing_name.clone(),
                address,
                date_from: candidate.date_from.to_string(),
                date_to: candidate.date_to.to_string(),
                door_access_code: door_code,
                wifi_ssid,
                wifi_password,
                check_in_instructions,
            };

            if enqueue_email(
                pool,
                &candidate.guest_email,
                &format!(
                    "Pre-Arrival Guide for Your Stay at {}",
                    candidate.listing_name
                ),
                common::email::EmailTemplate::PreArrivalGuideGuest.as_str(),
                &guest_payload,
            )
            .await
            {
                pre_arrival_count += 1;
            }
        }

        let claimed_host = db_core::notification_log::claim_and_log_notification(
            pool,
            candidate.booking_id,
            "host_upcoming_arrival",
            candidate.host_id,
        )
        .await
        .map_err(ApiError::Database)?;

        if claimed_host {
            let host_payload = common::email::HostUpcomingArrivalPayload {
                booking_id: candidate.booking_id,
                confirmation_code: candidate.confirmation_code.clone(),
                listing_name: candidate.listing_name.clone(),
                guest_name: format!(
                    "{} {}",
                    candidate.guest_first_name, candidate.guest_last_name
                ),
                number_of_persons: candidate.number_of_persons,
                date_from: candidate.date_from.to_string(),
                date_to: candidate.date_to.to_string(),
                door_access_code: candidate.door_access_code,
            };

            if enqueue_email(
                pool,
                &candidate.host_email,
                &format!("Guest Arrival in 48 Hours: {}", candidate.listing_name),
                common::email::EmailTemplate::HostUpcomingArrival.as_str(),
                &host_payload,
            )
            .await
            {
                pre_arrival_count += 1;
            }
        }
    }

    // Sweep Expiring Holds
    let hold_candidates = db_core::notification_log::get_expiring_hold_candidates(pool)
        .await
        .map_err(ApiError::Database)?;

    for candidate in hold_candidates {
        let claimed_hold = db_core::notification_log::claim_and_log_notification(
            pool,
            candidate.booking_id,
            "payment_hold_expiry_reminder",
            candidate.guest_id,
        )
        .await
        .map_err(ApiError::Database)?;

        if claimed_hold {
            let expires_at = candidate.expires_at;
            let hold_payload = common::email::PaymentHoldExpiryReminderPayload {
                booking_id: candidate.booking_id,
                confirmation_code: candidate.confirmation_code.clone(),
                listing_name: candidate.listing_name.clone(),
                expires_at: expires_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                total_price: candidate.total_price,
                currency: candidate.currency.clone(),
                checkout_url: format!("{}/bookings", guest_base_url()),
            };

            if enqueue_email(
                pool,
                &candidate.guest_email,
                &format!(
                    "Reminder: Your Reservation Hold for {} is Expiring Soon",
                    candidate.listing_name
                ),
                common::email::EmailTemplate::PaymentHoldExpiryReminder.as_str(),
                &hold_payload,
            )
            .await
            {
                hold_reminders_count += 1;
            }
        }
    }

    Ok(CronSweepResponse {
        status: "success".to_string(),
        pre_arrival_processed: pre_arrival_count,
        hold_reminders_processed: hold_reminders_count,
    })
}

/// Summary response returned by the scheduled complete stays cron job.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CompleteStaysResponse {
    /// Number of completed bookings transitioned.
    pub completed_count: usize,
    /// Identifiers of bookings marked completed and invoiced.
    pub booking_ids: Vec<Uuid>,
}

#[tracing::instrument(skip(req, pool, tax_settings, cron_settings))]
#[utoipa::path(
    patch,
    path = "/api/v1/internal/cron/complete-stays",
    tag = "bookings",
    responses(
        (status = 200, description = "Past checkout stays completed and invoiced", body = CompleteStaysResponse),
        (status = 401, description = "Unauthorized - invalid or missing cron secret"),
        (status = 500, description = "Internal server error")
    )
)]
/// Scheduled cron task endpoint that sweeps past-checkout confirmed bookings, marks them Completed,
/// issues statutory GCT tax invoices (INV-YYYY-{code}) excluding damage deposits, and notifies guests.
pub async fn complete_stays_cron(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    tax_settings: Option<web::Data<TaxSettings>>,
    cron_settings: Option<web::Data<CronSettings>>,
) -> Result<HttpResponse, ApiError> {
    verify_cron_secret(&req, cron_settings.as_ref())?;

    let today = (Utc::now() - chrono::Duration::hours(5)).date_naive();
    complete_stays_through(pool.get_ref(), tax_settings.as_ref(), today)
        .await
        .map(|stays| HttpResponse::Ok().json(stays))
}

/// Expires overdue change requests, then marks confirmed bookings that
/// checked out on or before `cutoff` as Completed (issuing their tax invoices
/// and notifying the guest). Completed bookings drop out of the query, so
/// repeated runs are no-ops.
async fn complete_stays_through(
    pool: &PgPool,
    tax_settings: Option<&web::Data<TaxSettings>>,
    today: NaiveDate,
) -> Result<CompleteStaysResponse, ApiError> {
    // Auto-expire any pending reservation change requests whose cutoff date has arrived
    if let Err(e) =
        db_core::reservation_change::expire_past_due_change_requests(pool, None, today).await
    {
        tracing::error!(error = %e, "Failed to expire past-due reservation change requests in cron");
    }

    let eligible = db_booking::get_confirmed_bookings_past_checkout(pool, today)
        .await
        .map_err(ApiError::Database)?;

    let supplier = tax_settings
        .as_ref()
        .map(|s| s.to_supplier_info())
        .or_else(|| TaxSettings::from_env().ok().map(|s| s.to_supplier_info()))
        .ok_or_else(|| {
            tracing::error!("Statutory tax supplier settings are missing or invalid");
            ApiError::Internal
        })?;

    let mut completed_ids = Vec::new();
    for booking in eligible {
        let old_booking = booking.clone();
        let update_params = db_core::models::UpdatedBooking {
            status: Some(BookingStatus::Completed),
            metadata: None,
            door_access_code: None,
            date_from: None,
            date_to: None,
            number_of_persons: None,
            daily_rate: None,
            total_days: None,
            sub_total_price: None,
            discount_value: None,
            tax_value: None,
            fee_breakdown: None,
            total_price: None,
            cleaning_fee: None,
            damage_deposit: None,
            platform_fee: None,
            tax_rate: None,
            exchange_rate: None,
        };

        match db_booking::update_booking(pool, booking.id, &update_params, &supplier).await {
            Ok(updated_b) => {
                completed_ids.push(updated_b.id);
                dispatch_booking_status_notifications(pool, old_booking, updated_b).await;
            }
            Err(e) => {
                tracing::error!(booking_id = %booking.id, error = %e, "Failed to complete stay in cron");
            }
        }
    }

    Ok(CompleteStaysResponse {
        completed_count: completed_ids.len(),
        booking_ids: completed_ids,
    })
}

/// Combined summary returned by the hourly sweeps endpoint.
#[derive(Debug, Serialize, ToSchema)]
pub struct CronSweepsResponse {
    /// Pending holds released because their `expires_at` had passed.
    pub holds_expired: usize,
    /// Result of the scheduled notification sweep.
    pub notifications: CronSweepResponse,
    /// Result of the stay completion sweep.
    pub stays: CompleteStaysResponse,
}

/// Last checkout date whose stays may be completed at `now`.
///
/// Stays complete at noon Jamaica time (UTC-5, no DST) on the checkout day,
/// after the guest has left. Before noon only earlier checkouts qualify, so
/// an hourly trigger keeps the noon semantics and catches up after a missed
/// run.
pub fn stay_completion_cutoff(now: chrono::DateTime<Utc>) -> NaiveDate {
    let jamaica = now - chrono::Duration::hours(5);
    let today = jamaica.date_naive();
    if chrono::Timelike::hour(&jamaica) >= 12 {
        today
    } else {
        today - chrono::Duration::days(1)
    }
}

#[tracing::instrument(skip(req, pool, tax_settings, cron_settings))]
#[utoipa::path(
    post,
    path = "/api/v1/internal/cron/run-sweeps",
    tag = "bookings",
    responses(
        (status = 200, description = "Notification and stay completion sweeps completed", body = CronSweepsResponse),
        (status = 401, description = "Unauthorized - invalid or missing cron secret"),
        (status = 500, description = "Internal server error")
    )
)]
/// Hourly cron endpoint for a single Cloud Scheduler job: runs the
/// notification sweep, then completes stays up to [`stay_completion_cutoff`].
pub async fn run_sweeps(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    tax_settings: Option<web::Data<TaxSettings>>,
    cron_settings: Option<web::Data<CronSettings>>,
) -> Result<HttpResponse, ApiError> {
    verify_cron_secret(&req, cron_settings.as_ref())?;
    let expired = db_booking::expire_stale_holds(pool.get_ref())
        .await
        .map_err(ApiError::Database)?;
    if !expired.is_empty() {
        tracing::info!(
            count = expired.len(),
            booking_ids = ?expired.iter().map(|b| b.id).collect::<Vec<_>>(),
            "released expired booking holds"
        );
    }
    let notifications = run_notification_sweep(pool.get_ref()).await?;
    let stays = complete_stays_through(
        pool.get_ref(),
        tax_settings.as_ref(),
        stay_completion_cutoff(Utc::now()),
    )
    .await?;
    Ok(HttpResponse::Ok().json(CronSweepsResponse {
        holds_expired: expired.len(),
        notifications,
        stays,
    }))
}
