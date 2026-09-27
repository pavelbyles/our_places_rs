use crate::provider::EmailProvider;
use db_core::PgPool;
use db_core::models::DbEmailStatus;
use std::time::Duration;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

/// Loads an external HTML template from disk using configured or standard search paths,
/// falling back to compile-time embedded copies if the file is unavailable.
pub fn load_template(filename: &str) -> String {
    let candidate_dirs = [
        std::env::var("EMAIL_TEMPLATES_DIR").ok(),
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
        "guest_host_message_notification.html" => {
            include_str!("../templates/guest_host_message_notification.html").to_string()
        }
        _ => include_str!("../templates/default_notification.html").to_string(),
    }
}

pub fn render_email_body(template_id: &str, payload: &serde_json::Value) -> String {
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
