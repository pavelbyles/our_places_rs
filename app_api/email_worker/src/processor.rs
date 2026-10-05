use crate::provider::EmailProvider;
use db_core::PgPool;
use db_core::models::DbEmailStatus;
use std::time::Duration;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

/// Loads an external HTML template from disk using configured or standard search paths,
/// falling back to compile-time embedded copies if the file is unavailable.
pub fn load_template(filename: &str) -> String {
    static EMAIL_TEMPLATES_DIR_ENV: &str = "EMAIL_TEMPLATES_DIR";
    let candidate_dirs = [
        std::env::var(EMAIL_TEMPLATES_DIR_ENV).ok(),
        Some("/app/templates".to_string()),
        Some("templates".to_string()),
        Some("app_api/email_worker/templates".to_string()),
    ];

    for dir in candidate_dirs.into_iter().flatten() {
        let path = std::path::Path::new(&dir).join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            return content;
        }
    }

    // Embedded fallback guarantees panic-free resilience in arbitrary execution contexts
    match filename {
        "user_verification_otp.html" => {
            include_str!("../templates/user_verification_otp.html").to_string()
        }
        "password_reset_otp.html" => {
            include_str!("../templates/password_reset_otp.html").to_string()
        }
        "booking_confirmation.html" => {
            include_str!("../templates/booking_confirmation.html").to_string()
        }
        "booking_confirmation_guest.html" => {
            include_str!("../templates/booking_confirmation_guest.html").to_string()
        }
        "booking_confirmation_host.html" => {
            include_str!("../templates/booking_confirmation_host.html").to_string()
        }
        "booking_updated_guest.html" => {
            include_str!("../templates/booking_updated_guest.html").to_string()
        }
        "booking_updated_host.html" => {
            include_str!("../templates/booking_updated_host.html").to_string()
        }
        "booking_cancelled_guest.html" => {
            include_str!("../templates/booking_cancelled_guest.html").to_string()
        }
        "booking_cancelled_host.html" => {
            include_str!("../templates/booking_cancelled_host.html").to_string()
        }
        "pre_arrival_guide_guest.html" => {
            include_str!("../templates/pre_arrival_guide_guest.html").to_string()
        }
        "host_upcoming_arrival.html" => {
            include_str!("../templates/host_upcoming_arrival.html").to_string()
        }
        "payment_hold_expiry_reminder.html" => {
            include_str!("../templates/payment_hold_expiry_reminder.html").to_string()
        }
        "guest_host_message_notification.html" => {
            include_str!("../templates/guest_host_message_notification.html").to_string()
        }
        _ => include_str!("../templates/default_notification.html").to_string(),
    }
}

/// Renders transactional email HTML templates with dynamic variable interpolation.
pub fn render_email_body(template_id: &str, payload: &serde_json::Value) -> String {
    let get_val = |key: &str| -> String {
        payload
            .get(key)
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => String::default(),
                other => other.to_string(),
            })
            .unwrap_or_default()
    };

    match template_id {
        "user_verification_otp" | "UserVerificationOtp" => {
            let code = payload
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("------");
            let template = load_template("user_verification_otp.html");
            template.replace("{{code}}", code).replace("{code}", code)
        }
        "password_reset_otp" | "PasswordResetOtp" => {
            let token = payload
                .get("token")
                .or_else(|| payload.get("code"))
                .and_then(|v| v.as_str())
                .unwrap_or("------");
            let template = load_template("password_reset_otp.html");
            template
                .replace("{{token}}", token)
                .replace("{token}", token)
                .replace("{{code}}", token)
                .replace("{code}", token)
        }
        "booking_confirmation" | "BookingConfirmation" => {
            let code = payload
                .get("confirmation_code")
                .and_then(|v| v.as_str())
                .unwrap_or("N/A");
            let dates = payload
                .get("dates")
                .and_then(|v| v.as_str())
                .unwrap_or("your stay");
            let total = payload
                .get("total_price")
                .and_then(|v| v.as_str())
                .unwrap_or("paid");
            let template = load_template("booking_confirmation.html");
            template
                .replace("{{confirmation_code}}", code)
                .replace("{confirmation_code}", code)
                .replace("{{dates}}", dates)
                .replace("{dates}", dates)
                .replace("{{total_price}}", total)
                .replace("{total_price}", total)
        }
        "booking_confirmation_guest" | "BookingConfirmationGuest" => {
            let template = load_template("booking_confirmation_guest.html");
            template
                .replace("{{confirmation_code}}", &get_val("confirmation_code"))
                .replace("{{listing_name}}", &get_val("listing_name"))
                .replace("{{guest_name}}", &get_val("guest_name"))
                .replace("{{date_from}}", &get_val("date_from"))
                .replace("{{date_to}}", &get_val("date_to"))
                .replace("{{total_price}}", &get_val("total_price"))
                .replace("{{currency}}", &get_val("currency"))
        }
        "booking_confirmation_host" | "BookingConfirmationHost" => {
            let template = load_template("booking_confirmation_host.html");
            template
                .replace("{{confirmation_code}}", &get_val("confirmation_code"))
                .replace("{{listing_name}}", &get_val("listing_name"))
                .replace("{{host_name}}", &get_val("host_name"))
                .replace("{{guest_name}}", &get_val("guest_name"))
                .replace("{{date_from}}", &get_val("date_from"))
                .replace("{{date_to}}", &get_val("date_to"))
                .replace("{{total_payout}}", &get_val("total_payout"))
                .replace("{{currency}}", &get_val("currency"))
        }
        "booking_updated_guest" | "BookingUpdatedGuest" => {
            let template = load_template("booking_updated_guest.html");
            template
                .replace("{{confirmation_code}}", &get_val("confirmation_code"))
                .replace("{{listing_name}}", &get_val("listing_name"))
                .replace("{{date_from}}", &get_val("date_from"))
                .replace("{{date_to}}", &get_val("date_to"))
                .replace("{{total_price}}", &get_val("total_price"))
                .replace("{{currency}}", &get_val("currency"))
                .replace("{{changes_summary}}", &get_val("changes_summary"))
        }
        "booking_updated_host" | "BookingUpdatedHost" => {
            let template = load_template("booking_updated_host.html");
            template
                .replace("{{confirmation_code}}", &get_val("confirmation_code"))
                .replace("{{listing_name}}", &get_val("listing_name"))
                .replace("{{guest_name}}", &get_val("guest_name"))
                .replace("{{date_from}}", &get_val("date_from"))
                .replace("{{date_to}}", &get_val("date_to"))
                .replace("{{total_payout}}", &get_val("total_payout"))
                .replace("{{currency}}", &get_val("currency"))
                .replace("{{changes_summary}}", &get_val("changes_summary"))
        }
        "booking_cancelled_guest" | "BookingCancelledGuest" => {
            let template = load_template("booking_cancelled_guest.html");
            template
                .replace("{{confirmation_code}}", &get_val("confirmation_code"))
                .replace("{{listing_name}}", &get_val("listing_name"))
                .replace("{{date_from}}", &get_val("date_from"))
                .replace("{{date_to}}", &get_val("date_to"))
                .replace("{{refund_amount}}", &get_val("refund_amount"))
                .replace("{{currency}}", &get_val("currency"))
                .replace("{{cancellation_policy}}", &get_val("cancellation_policy"))
        }
        "booking_cancelled_host" | "BookingCancelledHost" => {
            let template = load_template("booking_cancelled_host.html");
            template
                .replace("{{confirmation_code}}", &get_val("confirmation_code"))
                .replace("{{listing_name}}", &get_val("listing_name"))
                .replace("{{guest_name}}", &get_val("guest_name"))
                .replace("{{date_from}}", &get_val("date_from"))
                .replace("{{date_to}}", &get_val("date_to"))
                .replace("{{payout_impact}}", &get_val("payout_impact"))
                .replace("{{currency}}", &get_val("currency"))
        }
        "pre_arrival_guide_guest" | "PreArrivalGuideGuest" => {
            let template = load_template("pre_arrival_guide_guest.html");
            template
                .replace("{{confirmation_code}}", &get_val("confirmation_code"))
                .replace("{{listing_name}}", &get_val("listing_name"))
                .replace("{{address}}", &get_val("address"))
                .replace("{{date_from}}", &get_val("date_from"))
                .replace("{{date_to}}", &get_val("date_to"))
                .replace("{{door_access_code}}", &get_val("door_access_code"))
                .replace("{{wifi_ssid}}", &get_val("wifi_ssid"))
                .replace("{{wifi_password}}", &get_val("wifi_password"))
                .replace(
                    "{{check_in_instructions}}",
                    &get_val("check_in_instructions"),
                )
        }
        "host_upcoming_arrival" | "HostUpcomingArrival" => {
            let template = load_template("host_upcoming_arrival.html");
            template
                .replace("{{confirmation_code}}", &get_val("confirmation_code"))
                .replace("{{listing_name}}", &get_val("listing_name"))
                .replace("{{guest_name}}", &get_val("guest_name"))
                .replace("{{number_of_persons}}", &get_val("number_of_persons"))
                .replace("{{date_from}}", &get_val("date_from"))
                .replace("{{date_to}}", &get_val("date_to"))
                .replace("{{door_access_code}}", &get_val("door_access_code"))
        }
        "payment_hold_expiry_reminder" | "PaymentHoldExpiryReminder" => {
            let template = load_template("payment_hold_expiry_reminder.html");
            template
                .replace("{{confirmation_code}}", &get_val("confirmation_code"))
                .replace("{{listing_name}}", &get_val("listing_name"))
                .replace("{{expires_at}}", &get_val("expires_at"))
                .replace("{{total_price}}", &get_val("total_price"))
                .replace("{{currency}}", &get_val("currency"))
                .replace("{{checkout_url}}", &get_val("checkout_url"))
        }
        "guest_host_message_notification" | "GuestHostMessageNotification" => {
            let sender = payload
                .get("sender_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Someone");
            let message = payload
                .get("message_text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let template = load_template("guest_host_message_notification.html");
            template
                .replace("{{sender_name}}", sender)
                .replace("{sender_name}", sender)
                .replace("{{message_text}}", message)
                .replace("{message_text}", message)
        }
        _ => {
            let pretty_json = serde_json::to_string_pretty(payload).unwrap_or_default();
            let template = load_template("default_notification.html");
            template
                .replace("{{payload}}", &pretty_json)
                .replace("{payload}", &pretty_json)
        }
    }
}

/// Processes an individual outbox email event with idempotency, exponential backoff, and delivery status tracking.
#[instrument(skip(pool, provider))]
pub async fn process_email_event(
    pool: &PgPool,
    provider: &dyn EmailProvider,
    email_id: Uuid,
    max_retries: i32,
) -> Result<(), String> {
    // 1. Claim job & check idempotency in a short atomic DB transaction
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let record = db_core::email_outbox::get_email_outbox_for_update(&mut tx, email_id)
        .await
        .map_err(|e| e.to_string())?;

    let record = match record {
        Some(r) => r,
        None => {
            warn!("Email record {} not found in outbox", email_id);
            return Ok(());
        }
    };

    // Idempotency check: if already sent, exit cleanly without re-sending
    if record.status == DbEmailStatus::Sent {
        info!(
            "Email {} already sent. Skipping duplicate delivery.",
            email_id
        );
        return Ok(());
    }

    let mut attempt = record.attempts;
    let allowed_retries = if record.max_retries > 0 {
        record.max_retries
    } else {
        max_retries
    };

    db_core::email_outbox::mark_email_processing(&mut tx, email_id, attempt + 1)
        .await
        .map_err(|e| e.to_string())?;

    // Commit transaction to release row lock BEFORE network I/O
    tx.commit().await.map_err(|e| e.to_string())?;

    // 2. In-app retry execution loop (External Network I/O decoupled from DB locks)
    let mut last_err = None;
    let body = render_email_body(&record.template_id, &record.payload);

    while attempt < allowed_retries {
        attempt += 1;

        // Enforce strict 3-second timeout per SMTP dispatch attempt
        let send_fut = provider.send_email(&record.recipient_email, &record.subject, &body);
        match tokio::time::timeout(Duration::from_secs(3), send_fut).await {
            Ok(Ok(())) => {
                db_core::email_outbox::mark_email_sent(pool, email_id, attempt)
                    .await
                    .map_err(|e| e.to_string())?;
                info!(
                    "Email {} successfully dispatched on attempt {}",
                    email_id, attempt
                );
                return Ok(());
            }
            Ok(Err(e)) => {
                warn!("Attempt {} failed for email {}: {}", attempt, email_id, e);
                last_err = Some(e);
            }
            Err(_) => {
                let timeout_err = format!("Attempt {} timed out after 3 seconds", attempt);
                warn!("{}", timeout_err);
                last_err = Some(timeout_err);
            }
        }

        if attempt < allowed_retries {
            // Exponential backoff delay within application worker
            let delay_ms = 100u64.saturating_mul(2u64.pow((attempt as u32).min(6)));
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    // 3. Retries exhausted: mark as failed without re-queueing on Pub/Sub
    let err_msg = last_err.unwrap_or_else(|| "Max retries exceeded".into());
    db_core::email_outbox::mark_email_failed(pool, email_id, attempt, &err_msg)
        .await
        .map_err(|e| e.to_string())?;

    error!(
        "Email {} permanently failed after {} attempts: {}",
        email_id, allowed_retries, err_msg
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_pre_arrival_guide() {
        let payload = serde_json::json!({
            "confirmation_code": "PRE1234",
            "listing_name": "Villa Tropical",
            "address": "123 Ocean Blvd, Portland",
            "date_from": "2026-11-01",
            "date_to": "2026-11-05",
            "door_access_code": "5678",
            "wifi_ssid": "VillaGuestWifi",
            "wifi_password": "supersecretpassword",
            "check_in_instructions": "Gate code is 1234, door keypad on the left."
        });

        let body = render_email_body("pre_arrival_guide_guest", &payload);
        assert!(body.contains("PRE1234"));
        assert!(body.contains("Villa Tropical"));
        assert!(body.contains("5678"));
        assert!(body.contains("VillaGuestWifi"));
        assert!(body.contains("supersecretpassword"));
        assert!(body.contains("Gate code is 1234"));
    }

    #[test]
    fn test_render_host_upcoming_arrival() {
        let payload = serde_json::json!({
            "confirmation_code": "HOST999",
            "listing_name": "Villa Sunset",
            "guest_name": "Alice Smith",
            "number_of_persons": 4,
            "date_from": "2026-11-10",
            "date_to": "2026-11-15",
            "door_access_code": "9876"
        });

        let body = render_email_body("host_upcoming_arrival", &payload);
        assert!(body.contains("HOST999"));
        assert!(body.contains("Alice Smith"));
        assert!(body.contains("4 guests"));
        assert!(body.contains("9876"));
    }

    #[test]
    fn test_render_payment_hold_expiry_reminder() {
        let payload = serde_json::json!({
            "confirmation_code": "HOLD111",
            "listing_name": "Cozy Flat",
            "expires_at": "2026-11-01 14:00 UTC",
            "total_price": "450.00",
            "currency": "USD",
            "checkout_url": "https://ourplaces.co/checkout/HOLD111"
        });

        let body = render_email_body("payment_hold_expiry_reminder", &payload);
        assert!(body.contains("HOLD111"));
        assert!(body.contains("Cozy Flat"));
        assert!(body.contains("2026-11-01 14:00 UTC"));
        assert!(body.contains("https://ourplaces.co/checkout/HOLD111"));
    }
}
