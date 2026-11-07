use crate::provider::EmailProvider;
use db_core::PgPool;
use db_core::models::DbEmailStatus;
use std::time::Duration;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

/// Per-attempt SMTP send timeout. A remote relay (e.g. Gmail) can spend most
/// of 3s on the TLS handshake and login alone. The Pub/Sub ack deadline must
/// exceed the worst case (max retries x this + backoff), or a still-running
/// send is redelivered and the email goes out twice.
const SEND_TIMEOUT: Duration = Duration::from_secs(15);

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
        "booking_completed_guest.html" => {
            include_str!("../templates/booking_completed_guest.html").to_string()
        }
        "reservation_change_requested.html" => {
            include_str!("../templates/reservation_change_requested.html").to_string()
        }
        "reservation_change_approved.html" => {
            include_str!("../templates/reservation_change_approved.html").to_string()
        }
        "reservation_change_declined.html" => {
            include_str!("../templates/reservation_change_declined.html").to_string()
        }
        "request_notification.html" => {
            include_str!("../templates/request_notification.html").to_string()
        }
        _ => include_str!("../templates/default_notification.html").to_string(),
    }
}

/// The promo row for an email's price table. Empty when there's no promo, so a
/// booking without a code shows nothing extra.
fn promo_table_row(code: &str, discount: &str, currency: &str) -> String {
    if code.is_empty() {
        return String::new();
    }
    format!(
        r#"<tr style="border-bottom: 1px solid #f1f5f9;"><td style="padding: 8px 0;">Includes promo discount ({})</td><td style="padding: 8px 0; text-align: right;">-{} {}</td></tr>"#,
        escape_html(code),
        escape_html(currency),
        escape_html(discount)
    )
}

/// A host's payout line. Empty with no figure, so no blank amount is shown.
fn payout_paragraph(label: &str, amount: &str, currency: &str) -> String {
    if amount.is_empty() {
        return String::new();
    }
    format!(
        r#"<p style="margin: 0;"><strong>{label}:</strong> {} {}</p>"#,
        escape_html(currency),
        escape_html(amount)
    )
}

/// The promo line for a simple email. Empty when there's no promo.
fn promo_paragraph(code: &str, discount: &str, currency: &str) -> String {
    if code.is_empty() {
        return String::new();
    }
    format!(
        "<p>Includes promo discount ({}): -{} {}</p>",
        escape_html(code),
        escape_html(currency),
        escape_html(discount)
    )
}

/// A template renderer, keyed by its snake_case id and its legacy PascalCase id.
type Renderer = (&'static str, &'static str, fn(&serde_json::Value) -> String);

/// Every non-request template, each rendered by its own function.
const RENDERERS: &[Renderer] = &[
    (
        "user_verification_otp",
        "UserVerificationOtp",
        render_user_verification_otp,
    ),
    (
        "password_reset_otp",
        "PasswordResetOtp",
        render_password_reset_otp,
    ),
    (
        "booking_confirmation",
        "BookingConfirmation",
        render_booking_confirmation,
    ),
    (
        "booking_confirmation_guest",
        "BookingConfirmationGuest",
        render_booking_confirmation_guest,
    ),
    (
        "booking_confirmation_host",
        "BookingConfirmationHost",
        render_booking_confirmation_host,
    ),
    (
        "booking_updated_guest",
        "BookingUpdatedGuest",
        render_booking_updated_guest,
    ),
    (
        "booking_updated_host",
        "BookingUpdatedHost",
        render_booking_updated_host,
    ),
    (
        "booking_cancelled_guest",
        "BookingCancelledGuest",
        render_booking_cancelled_guest,
    ),
    (
        "booking_cancelled_host",
        "BookingCancelledHost",
        render_booking_cancelled_host,
    ),
    (
        "booking_completed_guest",
        "BookingCompletedGuest",
        render_booking_completed_guest,
    ),
    (
        "pre_arrival_guide_guest",
        "PreArrivalGuideGuest",
        render_pre_arrival_guide_guest,
    ),
    (
        "host_upcoming_arrival",
        "HostUpcomingArrival",
        render_host_upcoming_arrival,
    ),
    (
        "payment_hold_expiry_reminder",
        "PaymentHoldExpiryReminder",
        render_payment_hold_expiry_reminder,
    ),
    (
        "guest_host_message_notification",
        "GuestHostMessageNotification",
        render_guest_host_message_notification,
    ),
    (
        "reservation_change_requested",
        "ReservationChangeRequested",
        render_reservation_change_requested,
    ),
    (
        "reservation_change_approved",
        "ReservationChangeApproved",
        render_reservation_change_approved,
    ),
    (
        "reservation_change_declined",
        "ReservationChangeDeclined",
        render_reservation_change_declined,
    ),
];

/// Renders transactional email HTML templates with dynamic variable interpolation.
pub fn render_email_body(template_id: &str, payload: &serde_json::Value) -> String {
    crate::request_emails::render(template_id, payload)
        .or_else(|| {
            RENDERERS
                .iter()
                .find(|(snake, pascal, _)| *snake == template_id || *pascal == template_id)
                .map(|(_, _, render)| render(payload))
        })
        .unwrap_or_else(|| render_default(payload))
}

/// A payload value as display text; missing and null are empty.
fn val(payload: &serde_json::Value, key: &str) -> String {
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
}

/// A money field shown with the currency's symbol and thousands separators
/// (`$14,133.00`); templates put `{{currency}}` before it. Empty when the
/// field is absent, so optional lines can be left out. A value that is not a
/// number is shown as sent, with a warning.
fn amt(payload: &serde_json::Value, key: &str) -> String {
    let raw = val(payload, key);
    if raw.is_empty() {
        return raw;
    }
    match raw.parse::<rust_decimal::Decimal>() {
        Ok(amount) => common::money::item_amount(&val(payload, "currency"), amount),
        Err(e) => {
            tracing::warn!(key, value = %raw, error = %e, "email money field is not a number; shown as sent");
            raw
        }
    }
}

/// A money field with its currency code too (`USD $14,133.00`), for amounts
/// the template shows without `{{currency}}`. Empty when absent.
fn total_amt(payload: &serde_json::Value, key: &str) -> String {
    let shown = amt(payload, key);
    if shown.is_empty() {
        shown
    } else {
        format!("{} {shown}", val(payload, "currency"))
    }
}

/// Zero in the payload's currency (`$0.00`), for a balance or retained
/// amount the event left out.
fn zero_amt(payload: &serde_json::Value) -> String {
    common::money::item_amount(&val(payload, "currency"), rust_decimal::Decimal::ZERO)
}

fn render_user_verification_otp(payload: &serde_json::Value) -> String {
    let code = payload
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("------");
    let template = load_template("user_verification_otp.html");
    template.replace("{{code}}", code).replace("{code}", code)
}

fn render_password_reset_otp(payload: &serde_json::Value) -> String {
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

fn render_booking_confirmation(payload: &serde_json::Value) -> String {
    let code = payload
        .get("confirmation_code")
        .and_then(|v| v.as_str())
        .unwrap_or("N/A");
    let dates = payload
        .get("dates")
        .and_then(|v| v.as_str())
        .unwrap_or("your stay");
    let total_amount = total_amt(payload, "total_price");
    let total = if total_amount.is_empty() {
        "paid"
    } else {
        total_amount.as_str()
    };
    let template = load_template("booking_confirmation.html");
    template
        .replace(
            "{{promo_line}}",
            &promo_paragraph(
                &val(payload, "promo_code"),
                &amt(payload, "promo_discount"),
                &val(payload, "currency"),
            ),
        )
        .replace("{{confirmation_code}}", code)
        .replace("{confirmation_code}", code)
        .replace("{{dates}}", dates)
        .replace("{dates}", dates)
        .replace("{{total_price}}", total)
        .replace("{total_price}", total)
}

fn render_booking_confirmation_guest(payload: &serde_json::Value) -> String {
    let template = load_template("booking_confirmation_guest.html");
    let tax_rate_raw = val(payload, "tax_rate");
    let tax_rate_disp = if let Ok(dec) = tax_rate_raw.parse::<rust_decimal::Decimal>() {
        if dec < rust_decimal::Decimal::ONE && dec > rust_decimal::Decimal::ZERO {
            let pct = dec * rust_decimal::Decimal::from(100);
            let s = format!("{:.2}", pct);
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            let s = format!("{:.2}", dec);
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
    } else if !tax_rate_raw.is_empty() {
        tax_rate_raw
    } else {
        "15".to_string()
    };

    template
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace("{{listing_name}}", &val(payload, "listing_name"))
        .replace("{{guest_name}}", &val(payload, "guest_name"))
        .replace("{{date_from}}", &val(payload, "date_from"))
        .replace("{{date_to}}", &val(payload, "date_to"))
        .replace("{{total_price}}", &amt(payload, "total_price"))
        .replace("{{currency}}", &val(payload, "currency"))
        .replace("{{accommodation_net}}", &amt(payload, "accommodation_net"))
        .replace("{{cleaning_fee}}", &amt(payload, "cleaning_fee"))
        .replace("{{tax_rate}}", &tax_rate_disp)
        .replace("{{tax_amount}}", &amt(payload, "tax_amount"))
        .replace("{{platform_fee}}", &amt(payload, "platform_fee"))
        .replace("{{damage_deposit}}", &amt(payload, "damage_deposit"))
        .replace("{{receipt_url}}", &val(payload, "receipt_url"))
        .replace(
            "{{promo_line}}",
            &promo_table_row(
                &val(payload, "promo_code"),
                &amt(payload, "promo_discount"),
                &val(payload, "currency"),
            ),
        )
}

fn render_booking_confirmation_host(payload: &serde_json::Value) -> String {
    let template = load_template("booking_confirmation_host.html");
    template
        .replace(
            "{{payout_line}}",
            &payout_paragraph(
                "Host Payout",
                &amt(payload, "total_payout"),
                &val(payload, "currency"),
            ),
        )
        .replace(
            "{{promo_line}}",
            &promo_paragraph(
                &val(payload, "promo_code"),
                &amt(payload, "promo_discount"),
                &val(payload, "currency"),
            ),
        )
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace("{{listing_name}}", &val(payload, "listing_name"))
        .replace("{{host_name}}", &val(payload, "host_name"))
        .replace("{{guest_name}}", &val(payload, "guest_name"))
        .replace("{{date_from}}", &val(payload, "date_from"))
        .replace("{{date_to}}", &val(payload, "date_to"))
        .replace("{{total_payout}}", &amt(payload, "total_payout"))
        .replace("{{currency}}", &val(payload, "currency"))
}

fn render_booking_updated_guest(payload: &serde_json::Value) -> String {
    let template = load_template("booking_updated_guest.html");
    let balance_due_raw = amt(payload, "balance_due");
    let balance_due_disp = if balance_due_raw.is_empty() {
        zero_amt(payload)
    } else {
        balance_due_raw
    };
    template
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace("{{listing_name}}", &val(payload, "listing_name"))
        .replace("{{date_from}}", &val(payload, "date_from"))
        .replace("{{date_to}}", &val(payload, "date_to"))
        .replace("{{total_price}}", &amt(payload, "total_price"))
        .replace("{{balance_due}}", &balance_due_disp)
        .replace("{{currency}}", &val(payload, "currency"))
        .replace("{{changes_summary}}", &val(payload, "changes_summary"))
        .replace("{{receipt_url}}", &val(payload, "receipt_url"))
}

fn render_booking_updated_host(payload: &serde_json::Value) -> String {
    let template = load_template("booking_updated_host.html");
    template
        .replace(
            "{{payout_line}}",
            &payout_paragraph(
                "Adjusted Host Payout",
                &amt(payload, "total_payout"),
                &val(payload, "currency"),
            ),
        )
        .replace(
            "{{promo_line}}",
            &promo_paragraph(
                &val(payload, "promo_code"),
                &amt(payload, "promo_discount"),
                &val(payload, "currency"),
            ),
        )
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace("{{listing_name}}", &val(payload, "listing_name"))
        .replace("{{guest_name}}", &val(payload, "guest_name"))
        .replace("{{date_from}}", &val(payload, "date_from"))
        .replace("{{date_to}}", &val(payload, "date_to"))
        .replace("{{total_payout}}", &amt(payload, "total_payout"))
        .replace("{{currency}}", &val(payload, "currency"))
        .replace("{{changes_summary}}", &val(payload, "changes_summary"))
}

fn render_booking_cancelled_guest(payload: &serde_json::Value) -> String {
    let template = load_template("booking_cancelled_guest.html");
    let refund_pct_raw = val(payload, "refund_pct");
    let refund_pct_disp = if let Ok(dec) = refund_pct_raw.parse::<rust_decimal::Decimal>() {
        if dec <= rust_decimal::Decimal::ONE && dec >= rust_decimal::Decimal::ZERO {
            let pct = (dec * rust_decimal::Decimal::from(100)).round_dp(0);
            pct.to_string()
        } else {
            dec.round_dp(0).to_string()
        }
    } else if !refund_pct_raw.is_empty() {
        refund_pct_raw
    } else {
        "0".to_string()
    };
    let retained = amt(payload, "retained_amount");
    let retained_disp = if retained.is_empty() {
        zero_amt(payload)
    } else {
        retained
    };
    template
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace("{{listing_name}}", &val(payload, "listing_name"))
        .replace("{{date_from}}", &val(payload, "date_from"))
        .replace("{{date_to}}", &val(payload, "date_to"))
        .replace("{{refund_amount}}", &amt(payload, "refund_amount"))
        .replace("{{refund_pct}}", &refund_pct_disp)
        .replace("{{retained_amount}}", &retained_disp)
        .replace("{{currency}}", &val(payload, "currency"))
        .replace(
            "{{cancellation_policy}}",
            &val(payload, "cancellation_policy"),
        )
}

fn render_booking_cancelled_host(payload: &serde_json::Value) -> String {
    let template = load_template("booking_cancelled_host.html");
    template
        .replace(
            "{{payout_line}}",
            &payout_paragraph(
                "Net Payout Impact",
                &amt(payload, "payout_impact"),
                &val(payload, "currency"),
            ),
        )
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace("{{listing_name}}", &val(payload, "listing_name"))
        .replace("{{guest_name}}", &val(payload, "guest_name"))
        .replace("{{date_from}}", &val(payload, "date_from"))
        .replace("{{date_to}}", &val(payload, "date_to"))
        .replace("{{payout_impact}}", &amt(payload, "payout_impact"))
        .replace("{{currency}}", &val(payload, "currency"))
}

fn render_booking_completed_guest(payload: &serde_json::Value) -> String {
    let template = load_template("booking_completed_guest.html");
    template
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace("{{listing_name}}", &val(payload, "listing_name"))
        .replace("{{date_from}}", &val(payload, "date_from"))
        .replace("{{date_to}}", &val(payload, "date_to"))
        .replace("{{total_price}}", &amt(payload, "total_price"))
        .replace("{{currency}}", &val(payload, "currency"))
        .replace("{{invoice_url}}", &val(payload, "invoice_url"))
        .replace("{{invoice_number}}", &val(payload, "invoice_number"))
        .replace("{{accommodation_net}}", &amt(payload, "accommodation_net"))
        .replace("{{cleaning_fee}}", &amt(payload, "cleaning_fee"))
        .replace("{{tax_amount}}", &amt(payload, "tax_amount"))
}

fn render_pre_arrival_guide_guest(payload: &serde_json::Value) -> String {
    let template = load_template("pre_arrival_guide_guest.html");
    template
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace("{{listing_name}}", &val(payload, "listing_name"))
        .replace("{{address}}", &val(payload, "address"))
        .replace("{{date_from}}", &val(payload, "date_from"))
        .replace("{{date_to}}", &val(payload, "date_to"))
        .replace("{{door_access_code}}", &val(payload, "door_access_code"))
        .replace("{{wifi_ssid}}", &val(payload, "wifi_ssid"))
        .replace("{{wifi_password}}", &val(payload, "wifi_password"))
        .replace(
            "{{check_in_instructions}}",
            &val(payload, "check_in_instructions"),
        )
}

fn render_host_upcoming_arrival(payload: &serde_json::Value) -> String {
    let template = load_template("host_upcoming_arrival.html");
    template
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace("{{listing_name}}", &val(payload, "listing_name"))
        .replace("{{guest_name}}", &val(payload, "guest_name"))
        .replace("{{number_of_persons}}", &val(payload, "number_of_persons"))
        .replace("{{date_from}}", &val(payload, "date_from"))
        .replace("{{date_to}}", &val(payload, "date_to"))
        .replace("{{door_access_code}}", &val(payload, "door_access_code"))
}

fn render_payment_hold_expiry_reminder(payload: &serde_json::Value) -> String {
    let template = load_template("payment_hold_expiry_reminder.html");
    template
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace("{{listing_name}}", &val(payload, "listing_name"))
        .replace("{{expires_at}}", &val(payload, "expires_at"))
        .replace("{{total_price}}", &amt(payload, "total_price"))
        .replace("{{currency}}", &val(payload, "currency"))
        .replace("{{checkout_url}}", &val(payload, "checkout_url"))
}

fn render_guest_host_message_notification(payload: &serde_json::Value) -> String {
    let sender = payload
        .get("sender_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Someone");
    let message = payload
        .get("message_text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // Both are user-typed: escape them, or a message could inject HTML.
    let sender = escape_html(sender);
    let message = escape_html(message).replace('\n', "<br>");
    let template = load_template("guest_host_message_notification.html");
    template
        .replace("{{sender_name}}", &sender)
        .replace("{sender_name}", &sender)
        .replace("{{message_text}}", &message)
        .replace("{message_text}", &message)
}

fn render_reservation_change_requested(payload: &serde_json::Value) -> String {
    let template = load_template("reservation_change_requested.html");
    let reason_raw = val(payload, "reason");
    let reason_disp = if reason_raw.is_empty() {
        "No additional note provided.".to_string()
    } else {
        escape_html(&reason_raw)
    };
    template
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace(
            "{{listing_name}}",
            &escape_html(&val(payload, "listing_name")),
        )
        .replace(
            "{{recipient_name}}",
            &escape_html(&val(payload, "recipient_name")),
        )
        .replace(
            "{{initiator_name}}",
            &escape_html(&val(payload, "initiator_name")),
        )
        .replace("{{initiated_by_role}}", &val(payload, "initiated_by_role"))
        .replace(
            "{{previous_date_from}}",
            &val(payload, "previous_date_from"),
        )
        .replace("{{previous_date_to}}", &val(payload, "previous_date_to"))
        .replace(
            "{{proposed_date_from}}",
            &val(payload, "proposed_date_from"),
        )
        .replace("{{proposed_date_to}}", &val(payload, "proposed_date_to"))
        .replace(
            "{{previous_number_of_persons}}",
            &val(payload, "previous_number_of_persons"),
        )
        .replace(
            "{{proposed_number_of_persons}}",
            &val(payload, "proposed_number_of_persons"),
        )
        .replace(
            "{{previous_total_price}}",
            &amt(payload, "previous_total_price"),
        )
        .replace(
            "{{proposed_total_price}}",
            &amt(payload, "proposed_total_price"),
        )
        .replace("{{balance_due}}", &amt(payload, "balance_due"))
        .replace("{{currency}}", &val(payload, "currency"))
        .replace("{{reason}}", &reason_disp)
        .replace("{{action_url}}", &val(payload, "action_url"))
}

fn render_reservation_change_approved(payload: &serde_json::Value) -> String {
    let template = load_template("reservation_change_approved.html");
    let note_raw = val(payload, "response_note");
    let note_disp = if note_raw.is_empty() {
        "No additional note provided.".to_string()
    } else {
        escape_html(&note_raw)
    };
    template
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace(
            "{{listing_name}}",
            &escape_html(&val(payload, "listing_name")),
        )
        .replace(
            "{{recipient_name}}",
            &escape_html(&val(payload, "recipient_name")),
        )
        .replace(
            "{{responder_name}}",
            &escape_html(&val(payload, "responder_name")),
        )
        .replace("{{date_from}}", &val(payload, "date_from"))
        .replace("{{date_to}}", &val(payload, "date_to"))
        .replace("{{number_of_persons}}", &val(payload, "number_of_persons"))
        .replace("{{total_price}}", &amt(payload, "total_price"))
        .replace("{{balance_due}}", &amt(payload, "balance_due"))
        .replace("{{currency}}", &val(payload, "currency"))
        .replace("{{settlement_status}}", &val(payload, "settlement_status"))
        .replace("{{response_note}}", &note_disp)
        .replace("{{receipt_url}}", &val(payload, "receipt_url"))
}

fn render_reservation_change_declined(payload: &serde_json::Value) -> String {
    let template = load_template("reservation_change_declined.html");
    let note_raw = val(payload, "response_note");
    let note_disp = if note_raw.is_empty() {
        "No additional note provided.".to_string()
    } else {
        escape_html(&note_raw)
    };
    template
        .replace("{{confirmation_code}}", &val(payload, "confirmation_code"))
        .replace(
            "{{listing_name}}",
            &escape_html(&val(payload, "listing_name")),
        )
        .replace(
            "{{recipient_name}}",
            &escape_html(&val(payload, "recipient_name")),
        )
        .replace(
            "{{responder_name}}",
            &escape_html(&val(payload, "responder_name")),
        )
        .replace(
            "{{proposed_date_from}}",
            &val(payload, "proposed_date_from"),
        )
        .replace("{{proposed_date_to}}", &val(payload, "proposed_date_to"))
        .replace(
            "{{proposed_number_of_persons}}",
            &val(payload, "proposed_number_of_persons"),
        )
        .replace("{{response_note}}", &note_disp)
        .replace("{{booking_url}}", &val(payload, "booking_url"))
}

fn render_default(payload: &serde_json::Value) -> String {
    let pretty_json = serde_json::to_string_pretty(payload).unwrap_or_default();
    let template = load_template("default_notification.html");
    template
        .replace("{{payload}}", &pretty_json)
        .replace("{payload}", &pretty_json)
}

/// Escapes text for HTML element content and attribute values.
pub(crate) fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
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

        // Bound each SMTP dispatch attempt so a hung relay cannot stall the worker
        let send_fut = provider.send_email(&record.recipient_email, &record.subject, &body);
        match tokio::time::timeout(SEND_TIMEOUT, send_fut).await {
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
                let timeout_err = format!(
                    "Attempt {} timed out after {} seconds",
                    attempt,
                    SEND_TIMEOUT.as_secs()
                );
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
    #[test]
    fn message_notification_escapes_sender_and_text() {
        let body = super::render_email_body(
            "guest_host_message_notification",
            &serde_json::json!({
                "sender_name": "<b>Sam</b>",
                "message_text": "<script>alert(1)</script>\nsecond line",
            }),
        );
        assert!(!body.contains("<script>"));
        assert!(!body.contains("<b>Sam</b>"));
        assert!(body.contains("&lt;script&gt;alert(1)&lt;/script&gt;<br>second line"));
    }

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

    #[test]
    fn test_render_booking_cancelled_guest() {
        let payload = serde_json::json!({
            "confirmation_code": "CANCEL123",
            "listing_name": "Azure Haven",
            "date_from": "2026-12-01",
            "date_to": "2026-12-05",
            "refund_amount": "500.00",
            "currency": "USD",
            "cancellation_policy": "Moderate",
            "refund_pct": "0.50",
            "retained_amount": "327.50"
        });

        let body = render_email_body("booking_cancelled_guest", &payload);
        assert!(body.contains("CANCEL123"));
        assert!(body.contains("Azure Haven"));
        assert!(body.contains("50%"));
        assert!(body.contains("USD $500.00"));
        assert!(body.contains("USD $327.50"));
    }

    #[test]
    fn test_render_booking_updated_guest() {
        let payload = serde_json::json!({
            "confirmation_code": "UPDATE123",
            "listing_name": "Azure Haven",
            "date_from": "2026-12-05",
            "date_to": "2026-12-10",
            "total_price": "1200.00",
            "currency": "USD",
            "changes_summary": "Dates extended by 2 nights",
            "receipt_url": "https://ourplaces.co/bookings/123/documents/REC-2026-UPDATE123-R2",
            "balance_due": "350.00"
        });

        let body = render_email_body("booking_updated_guest", &payload);
        assert!(body.contains("UPDATE123"));
        assert!(body.contains("Azure Haven"));
        assert!(body.contains("USD $1,200.00"));
        assert!(body.contains("USD $350.00"));
        assert!(body.contains("https://ourplaces.co/bookings/123/documents/REC-2026-UPDATE123-R2"));
    }

    #[test]
    fn test_render_booking_completed_guest() {
        let payload = serde_json::json!({
            "confirmation_code": "COMP123",
            "listing_name": "Azure Haven",
            "date_from": "2026-12-01",
            "date_to": "2026-12-05",
            "total_price": "1000.00",
            "currency": "USD",
            "invoice_url": "https://ourplaces.co/bookings/123/documents/INV-2026-COMP123",
            "invoice_number": "INV-2026-COMP123",
            "accommodation_net": "800.00",
            "cleaning_fee": "50.00",
            "tax_amount": "127.50"
        });

        let body = render_email_body("booking_completed_guest", &payload);
        assert!(body.contains("COMP123"));
        assert!(body.contains("Azure Haven"));
        assert!(body.contains("INV-2026-COMP123"));
        assert!(body.contains("USD $1,000.00"));
        assert!(body.contains("USD $800.00"));
        assert!(body.contains("USD $50.00"));
        assert!(body.contains("USD $127.50"));
        assert!(body.contains("https://ourplaces.co/bookings/123/documents/INV-2026-COMP123"));
    }

    #[test]
    fn test_render_reservation_change_templates() {
        let requested_payload = serde_json::json!({
            "booking_id": Uuid::new_v4(),
            "change_request_id": Uuid::new_v4(),
            "confirmation_code": "CHG1234",
            "listing_name": "Villa Coral",
            "recipient_name": "Marcus Host",
            "initiator_name": "Elena Guest",
            "initiated_by_role": "guest",
            "previous_date_from": "2026-12-01",
            "previous_date_to": "2026-12-04",
            "proposed_date_from": "2026-12-01",
            "proposed_date_to": "2026-12-06",
            "previous_number_of_persons": 2,
            "proposed_number_of_persons": 4,
            "previous_total_price": "800.00",
            "proposed_total_price": "1350.00",
            "balance_due": "550.00",
            "currency": "USD",
            "reason": "Friends joining <script>alert(1)</script>",
            "action_url": "https://ourplaces.co/admin/bookings"
        });
        let req_body = render_email_body("reservation_change_requested", &requested_payload);
        assert!(req_body.contains("CHG1234"));
        assert!(req_body.contains("Villa Coral"));
        assert!(req_body.contains("Marcus Host"));
        assert!(req_body.contains("Elena Guest"));
        assert!(req_body.contains("2026-12-06"));
        assert!(req_body.contains("USD $550.00"));
        assert!(req_body.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!req_body.contains("{{"));
        assert!(!req_body.contains("}}"));

        let approved_payload = serde_json::json!({
            "booking_id": Uuid::new_v4(),
            "change_request_id": Uuid::new_v4(),
            "confirmation_code": "CHG1234",
            "listing_name": "Villa Coral",
            "recipient_name": "Elena Guest",
            "responder_name": "Marcus Host",
            "date_from": "2026-12-01",
            "date_to": "2026-12-06",
            "number_of_persons": 4,
            "total_price": "1350.00",
            "balance_due": "550.00",
            "currency": "USD",
            "settlement_status": "charged",
            "response_note": "Happy to host your friends!",
            "receipt_url": "https://ourplaces.co/bookings/123/documents/REC-2026-CHG1234-R2"
        });
        let app_body = render_email_body("reservation_change_approved", &approved_payload);
        assert!(app_body.contains("CHG1234"));
        assert!(app_body.contains("Happy to host your friends!"));
        assert!(app_body.contains("REC-2026-CHG1234-R2"));
        assert!(!app_body.contains("{{"));
        assert!(!app_body.contains("}}"));

        let declined_payload = serde_json::json!({
            "booking_id": Uuid::new_v4(),
            "change_request_id": Uuid::new_v4(),
            "confirmation_code": "CHG1234",
            "listing_name": "Villa Coral",
            "recipient_name": "Elena Guest",
            "responder_name": "Marcus Host",
            "proposed_date_from": "2026-12-01",
            "proposed_date_to": "2026-12-06",
            "proposed_number_of_persons": 4,
            "response_note": "Maintenance scheduled after Dec 4",
            "booking_url": "https://ourplaces.co/bookings"
        });
        let dec_body = render_email_body("reservation_change_declined", &declined_payload);
        assert!(dec_body.contains("CHG1234"));
        assert!(dec_body.contains("Maintenance scheduled after Dec 4"));
        assert!(dec_body.contains("https://ourplaces.co/bookings"));
        assert!(!dec_body.contains("{{"));
        assert!(!dec_body.contains("}}"));
    }
}
