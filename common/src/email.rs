use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;
use uuid::Uuid;

/// Status of an outbox email notification record during delivery lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailStatus {
    /// Email is enqueued in the outbox table awaiting worker dispatch.
    Pending,
    /// Email is currently being dispatched by a worker process.
    Processing,
    /// Email has been successfully accepted by the SMTP provider.
    Sent,
    /// Email delivery failed after exhausting all configured retry attempts.
    Failed,
}

impl fmt::Display for EmailStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Sent => write!(f, "sent"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for EmailStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "sent" => Ok(Self::Sent),
            "failed" => Ok(Self::Failed),
            other => Err(format!("Unknown email status: {}", other)),
        }
    }
}

/// Cloud Pub/Sub event emitted to notify background workers of a newly created outbox email.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailNotificationEvent {
    /// Primary key identifier of the outbox record to be processed.
    pub email_id: Uuid,
}

/// Identifiers for transactional email templates supported by the notification pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailTemplate {
    /// One-time password verification email for new user registration.
    UserVerificationOtp,
    /// One-time password code for account password reset.
    PasswordResetOtp,
    /// Generic booking confirmation notification.
    BookingConfirmation,
    /// Comprehensive booking confirmation notification delivered to the guest.
    BookingConfirmationGuest,
    /// Booking notification and payout details delivered to the host.
    BookingConfirmationHost,
    /// Notification delivered to the guest when booking dates or details are updated.
    BookingUpdatedGuest,
    /// Notification delivered to the host when guest modifies booking dates or party size.
    BookingUpdatedHost,
    /// Notification and refund details delivered to guest upon booking cancellation.
    BookingCancelledGuest,
    /// Notification delivered to host upon guest or administrative cancellation.
    BookingCancelledHost,
    /// Pre-arrival instructions including door access PIN and WiFi credentials for guest.
    PreArrivalGuideGuest,
    /// Reminder delivered to host before guest check-in date.
    HostUpcomingArrival,
    /// Reminder notification before an unconfirmed pending booking hold expires.
    PaymentHoldExpiryReminder,
    /// In-app chat notification delivered when a counterparty sends a new message.
    GuestHostMessageNotification,
}

impl EmailTemplate {
    /// Returns the static template identifier string matching filesystem HTML template filenames.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserVerificationOtp => "user_verification_otp",
            Self::PasswordResetOtp => "password_reset_otp",
            Self::BookingConfirmation => "booking_confirmation",
            Self::BookingConfirmationGuest => "booking_confirmation_guest",
            Self::BookingConfirmationHost => "booking_confirmation_host",
            Self::BookingUpdatedGuest => "booking_updated_guest",
            Self::BookingUpdatedHost => "booking_updated_host",
            Self::BookingCancelledGuest => "booking_cancelled_guest",
            Self::BookingCancelledHost => "booking_cancelled_host",
            Self::PreArrivalGuideGuest => "pre_arrival_guide_guest",
            Self::HostUpcomingArrival => "host_upcoming_arrival",
            Self::PaymentHoldExpiryReminder => "payment_hold_expiry_reminder",
            Self::GuestHostMessageNotification => "guest_host_message_notification",
        }
    }
}

impl fmt::Display for EmailTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for EmailTemplate {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user_verification_otp" => Ok(Self::UserVerificationOtp),
            "password_reset_otp" => Ok(Self::PasswordResetOtp),
            "booking_confirmation" => Ok(Self::BookingConfirmation),
            "booking_confirmation_guest" => Ok(Self::BookingConfirmationGuest),
            "booking_confirmation_host" => Ok(Self::BookingConfirmationHost),
            "booking_updated_guest" => Ok(Self::BookingUpdatedGuest),
            "booking_updated_host" => Ok(Self::BookingUpdatedHost),
            "booking_cancelled_guest" => Ok(Self::BookingCancelledGuest),
            "booking_cancelled_host" => Ok(Self::BookingCancelledHost),
            "pre_arrival_guide_guest" => Ok(Self::PreArrivalGuideGuest),
            "host_upcoming_arrival" => Ok(Self::HostUpcomingArrival),
            "payment_hold_expiry_reminder" => Ok(Self::PaymentHoldExpiryReminder),
            "guest_host_message_notification" => Ok(Self::GuestHostMessageNotification),
            other => Err(format!("Unknown email template: {}", other)),
        }
    }
}

// ============================================================================
// Strongly-Typed Transactional Email Payloads (Zero-Float Guarantee)
// ============================================================================

/// Strongly-typed email payload for guest booking confirmation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingConfirmationGuestPayload {
    /// Unique booking identifier.
    pub booking_id: Uuid,
    /// Human-readable booking confirmation code.
    pub confirmation_code: String,
    /// Name of booked property.
    pub listing_name: String,
    /// Check-in date.
    pub date_from: String,
    /// Check-out date.
    pub date_to: String,
    /// Total price charged.
    pub total_price: Decimal,
    /// Currency code.
    pub currency: String,
    /// Full name of primary guest.
    pub guest_name: String,
}

/// Strongly-typed email payload for host booking confirmation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingConfirmationHostPayload {
    /// Unique booking identifier.
    pub booking_id: Uuid,
    /// Human-readable booking confirmation code.
    pub confirmation_code: String,
    /// Name of booked property.
    pub listing_name: String,
    /// Check-in date.
    pub date_from: String,
    /// Check-out date.
    pub date_to: String,
    /// Total payout to host after platform commissions.
    pub total_payout: Decimal,
    /// Currency code.
    pub currency: String,
    /// Full name of primary guest.
    pub guest_name: String,
    /// Full name of property host.
    pub host_name: String,
}

/// Strongly-typed email payload for guest booking modifications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingUpdatedGuestPayload {
    /// Unique booking identifier.
    pub booking_id: Uuid,
    /// Human-readable booking confirmation code.
    pub confirmation_code: String,
    /// Name of booked property.
    pub listing_name: String,
    /// Check-in date.
    pub date_from: String,
    /// Check-out date.
    pub date_to: String,
    /// Updated total price.
    pub total_price: Decimal,
    /// Currency code.
    pub currency: String,
    /// Summary of changes applied.
    pub changes_summary: String,
}

/// Strongly-typed email payload for host booking modifications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingUpdatedHostPayload {
    /// Unique booking identifier.
    pub booking_id: Uuid,
    /// Human-readable booking confirmation code.
    pub confirmation_code: String,
    /// Name of booked property.
    pub listing_name: String,
    /// Check-in date.
    pub date_from: String,
    /// Check-out date.
    pub date_to: String,
    /// Updated total payout to host.
    pub total_payout: Decimal,
    /// Currency code.
    pub currency: String,
    /// Full name of primary guest.
    pub guest_name: String,
    /// Summary of changes applied.
    pub changes_summary: String,
}

/// Strongly-typed email payload for guest booking cancellation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingCancelledGuestPayload {
    /// Unique booking identifier.
    pub booking_id: Uuid,
    /// Human-readable booking confirmation code.
    pub confirmation_code: String,
    /// Name of booked property.
    pub listing_name: String,
    /// Check-in date.
    pub date_from: String,
    /// Check-out date.
    pub date_to: String,
    /// Amount refunded to guest.
    pub refund_amount: Decimal,
    /// Currency code.
    pub currency: String,
    /// Applied cancellation policy identifier.
    pub cancellation_policy: String,
}

/// Strongly-typed email payload for host booking cancellation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingCancelledHostPayload {
    /// Unique booking identifier.
    pub booking_id: Uuid,
    /// Human-readable booking confirmation code.
    pub confirmation_code: String,
    /// Name of booked property.
    pub listing_name: String,
    /// Check-in date.
    pub date_from: String,
    /// Check-out date.
    pub date_to: String,
    /// Full name of primary guest.
    pub guest_name: String,
    /// Net financial payout impact on host.
    pub payout_impact: Decimal,
    /// Currency code.
    pub currency: String,
}

/// Strongly-typed email payload for guest pre-arrival access guide.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreArrivalGuideGuestPayload {
    /// Unique booking identifier.
    pub booking_id: Uuid,
    /// Human-readable booking confirmation code.
    pub confirmation_code: String,
    /// Name of booked property.
    pub listing_name: String,
    /// Property physical street address.
    pub address: String,
    /// Check-in date.
    pub date_from: String,
    /// Check-out date.
    pub date_to: String,
    /// Door lock pin or keycode.
    pub door_access_code: String,
    /// Local WiFi network SSID.
    pub wifi_ssid: Option<String>,
    /// Local WiFi password.
    pub wifi_password: Option<String>,
    /// Specific check-in guidance from host.
    pub check_in_instructions: Option<String>,
}

/// Strongly-typed email payload for upcoming guest arrival notification to host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostUpcomingArrivalPayload {
    /// Unique booking identifier.
    pub booking_id: Uuid,
    /// Human-readable booking confirmation code.
    pub confirmation_code: String,
    /// Name of booked property.
    pub listing_name: String,
    /// Full name of arriving guest.
    pub guest_name: String,
    /// Number of guests in traveling party.
    pub number_of_persons: i32,
    /// Check-in date.
    pub date_from: String,
    /// Check-out date.
    pub date_to: String,
    /// Issued door access code if available.
    pub door_access_code: Option<String>,
}

/// Strongly-typed email payload for pending hold expiration reminder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentHoldExpiryReminderPayload {
    /// Unique booking identifier.
    pub booking_id: Uuid,
    /// Human-readable booking confirmation code.
    pub confirmation_code: String,
    /// Name of held property.
    pub listing_name: String,
    /// Timestamp when reservation hold expires.
    pub expires_at: String,
    /// Total reservation price.
    pub total_price: Decimal,
    /// Currency code.
    pub currency: String,
    /// Direct URL to complete checkout.
    pub checkout_url: String,
}

// ============================================================================
// Type State Pattern for Compile-Time Guaranteed Email State Transitions
// ============================================================================

/// Marker type indicating an email record is pending initial delivery attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingState;

/// Marker type indicating an email record is actively being processed by a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessingState;

/// Marker type indicating an email record has successfully completed delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SentState;

/// Marker type indicating an email record has permanently failed delivery after retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailedState;

/// Strongly-typed outbox email entity enforcing compile-time valid state machine transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailRecord<State> {
    /// Unique outbox email record identifier.
    pub id: Uuid,
    /// Target email recipient.
    pub recipient_email: String,
    /// Subject header line.
    pub subject: String,
    /// Template identifier.
    pub template_id: String,
    /// Template interpolation payload.
    pub payload: serde_json::Value,
    /// Number of delivery attempts made.
    pub attempts: i32,
    /// Maximum permitted retry attempts.
    pub max_retries: i32,
    /// Last delivery error message if any attempt failed.
    pub last_error: Option<String>,
    _state: PhantomData<State>,
}

impl EmailRecord<PendingState> {
    /// Constructs a new pending email record with initial zero retry count.
    pub fn new(
        id: Uuid,
        recipient_email: String,
        subject: String,
        template_id: String,
        payload: serde_json::Value,
        max_retries: i32,
    ) -> Self {
        Self {
            id,
            recipient_email,
            subject,
            template_id,
            payload,
            attempts: 0,
            max_retries,
            last_error: None,
            _state: PhantomData,
        }
    }

    /// Transition from Pending -> Processing (increments attempts)
    pub fn start_processing(self) -> EmailRecord<ProcessingState> {
        EmailRecord {
            id: self.id,
            recipient_email: self.recipient_email,
            subject: self.subject,
            template_id: self.template_id,
            payload: self.payload,
            attempts: self.attempts + 1,
            max_retries: self.max_retries,
            last_error: self.last_error,
            _state: PhantomData,
        }
    }
}

impl EmailRecord<ProcessingState> {
    /// Transition from Processing -> Sent
    pub fn mark_sent(self) -> EmailRecord<SentState> {
        EmailRecord {
            id: self.id,
            recipient_email: self.recipient_email,
            subject: self.subject,
            template_id: self.template_id,
            payload: self.payload,
            attempts: self.attempts,
            max_retries: self.max_retries,
            last_error: None,
            _state: PhantomData,
        }
    }

    /// Transition from Processing -> Failed
    pub fn mark_failed(self, error_message: String) -> EmailRecord<FailedState> {
        EmailRecord {
            id: self.id,
            recipient_email: self.recipient_email,
            subject: self.subject,
            template_id: self.template_id,
            payload: self.payload,
            attempts: self.attempts,
            max_retries: self.max_retries,
            last_error: Some(error_message),
            _state: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_notification_event_serde() {
        let id = Uuid::new_v4();
        let event = EmailNotificationEvent { email_id: id };
        let json = serde_json::to_string(&event).expect("Serialization failed");
        let deserialized: EmailNotificationEvent =
            serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_email_status_serde_and_display() {
        let statuses = [
            (EmailStatus::Pending, "\"pending\"", "pending"),
            (EmailStatus::Processing, "\"processing\"", "processing"),
            (EmailStatus::Sent, "\"sent\"", "sent"),
            (EmailStatus::Failed, "\"failed\"", "failed"),
        ];

        for (status, expected_json, expected_str) in statuses {
            let json = serde_json::to_string(&status).expect("Serialization failed");
            assert_eq!(json, expected_json);
            let parsed: EmailStatus = serde_json::from_str(&json).expect("Deserialization failed");
            assert_eq!(parsed, status);
            assert_eq!(status.to_string(), expected_str);
            assert_eq!(expected_str.parse::<EmailStatus>().unwrap(), status);
        }
    }

    #[test]
    fn test_email_template_display_and_as_str() {
        let expected = [
            (EmailTemplate::UserVerificationOtp, "user_verification_otp"),
            (EmailTemplate::PasswordResetOtp, "password_reset_otp"),
            (EmailTemplate::BookingConfirmation, "booking_confirmation"),
            (
                EmailTemplate::BookingConfirmationGuest,
                "booking_confirmation_guest",
            ),
            (
                EmailTemplate::BookingConfirmationHost,
                "booking_confirmation_host",
            ),
            (EmailTemplate::BookingUpdatedGuest, "booking_updated_guest"),
            (EmailTemplate::BookingUpdatedHost, "booking_updated_host"),
            (
                EmailTemplate::BookingCancelledGuest,
                "booking_cancelled_guest",
            ),
            (
                EmailTemplate::BookingCancelledHost,
                "booking_cancelled_host",
            ),
            (
                EmailTemplate::PreArrivalGuideGuest,
                "pre_arrival_guide_guest",
            ),
            (EmailTemplate::HostUpcomingArrival, "host_upcoming_arrival"),
            (
                EmailTemplate::PaymentHoldExpiryReminder,
                "payment_hold_expiry_reminder",
            ),
            (
                EmailTemplate::GuestHostMessageNotification,
                "guest_host_message_notification",
            ),
        ];

        for (variant, str_val) in expected {
            assert_eq!(variant.as_str(), str_val);
            assert_eq!(variant.to_string(), str_val);
            assert_eq!(str_val.parse::<EmailTemplate>().unwrap(), variant);
        }
    }

    #[test]
    fn test_email_payloads_serde() {
        let guest_confirm = BookingConfirmationGuestPayload {
            booking_id: Uuid::new_v4(),
            confirmation_code: "CONF123".into(),
            listing_name: "Villa Ocean".into(),
            date_from: "2026-11-01".into(),
            date_to: "2026-11-05".into(),
            total_price: Decimal::new(125050, 2),
            currency: "USD".into(),
            guest_name: "John Doe".into(),
        };
        let json = serde_json::to_string(&guest_confirm).unwrap();
        let parsed: BookingConfirmationGuestPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(guest_confirm, parsed);

        let pre_arrival = PreArrivalGuideGuestPayload {
            booking_id: Uuid::new_v4(),
            confirmation_code: "CONF123".into(),
            listing_name: "Villa Ocean".into(),
            address: "123 Coastal Way, Port Antonio".into(),
            date_from: "2026-11-01".into(),
            date_to: "2026-11-05".into(),
            door_access_code: "4829".into(),
            wifi_ssid: Some("VillaGuest".into()),
            wifi_password: Some("secret123".into()),
            check_in_instructions: Some("Keypad is on the front gate".into()),
        };
        let json = serde_json::to_string(&pre_arrival).unwrap();
        let parsed: PreArrivalGuideGuestPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(pre_arrival, parsed);
    }

    #[test]
    fn test_type_state_pending_to_processing_to_sent() {
        let id = Uuid::new_v4();
        let pending = EmailRecord::new(
            id,
            "guest@example.com".into(),
            "Welcome".into(),
            EmailTemplate::UserVerificationOtp.as_str().into(),
            serde_json::json!({ "code": "123456" }),
            3,
        );
        assert_eq!(pending.attempts, 0);

        let processing = pending.start_processing();
        assert_eq!(processing.attempts, 1);
        assert_eq!(processing.id, id);

        let sent = processing.mark_sent();
        assert_eq!(sent.attempts, 1);
        assert_eq!(sent.last_error, None);
        assert_eq!(sent.id, id);
    }

    #[test]
    fn test_type_state_pending_to_processing_to_failed() {
        let id = Uuid::new_v4();
        let pending = EmailRecord::new(
            id,
            "guest@example.com".into(),
            "Welcome".into(),
            EmailTemplate::PasswordResetOtp.as_str().into(),
            serde_json::json!({ "token": "xyz" }),
            3,
        );

        let processing = pending.start_processing();
        assert_eq!(processing.attempts, 1);

        let failed = processing.mark_failed("SMTP Connection Timeout".into());
        assert_eq!(failed.attempts, 1);
        assert_eq!(
            failed.last_error.as_deref(),
            Some("SMTP Connection Timeout")
        );
    }
}
