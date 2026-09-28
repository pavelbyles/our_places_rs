use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailStatus {
    Pending,
    Processing,
    Sent,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailNotificationEvent {
    pub email_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailTemplate {
    UserVerificationOtp,
    PasswordResetOtp,
    BookingConfirmation,
    BookingConfirmationGuest,
    BookingConfirmationHost,
    BookingUpdatedGuest,
    BookingUpdatedHost,
    BookingCancelledGuest,
    BookingCancelledHost,
    PreArrivalGuideGuest,
    HostUpcomingArrival,
    PaymentHoldExpiryReminder,
    GuestHostMessageNotification,
}

impl EmailTemplate {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingConfirmationGuestPayload {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub listing_name: String,
    pub date_from: String,
    pub date_to: String,
    pub total_price: Decimal,
    pub currency: String,
    pub guest_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingConfirmationHostPayload {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub listing_name: String,
    pub date_from: String,
    pub date_to: String,
    pub total_payout: Decimal,
    pub currency: String,
    pub guest_name: String,
    pub host_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingUpdatedGuestPayload {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub listing_name: String,
    pub date_from: String,
    pub date_to: String,
    pub total_price: Decimal,
    pub currency: String,
    pub changes_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingUpdatedHostPayload {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub listing_name: String,
    pub date_from: String,
    pub date_to: String,
    pub total_payout: Decimal,
    pub currency: String,
    pub guest_name: String,
    pub changes_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingCancelledGuestPayload {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub listing_name: String,
    pub date_from: String,
    pub date_to: String,
    pub refund_amount: Decimal,
    pub currency: String,
    pub cancellation_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookingCancelledHostPayload {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub listing_name: String,
    pub date_from: String,
    pub date_to: String,
    pub guest_name: String,
    pub payout_impact: Decimal,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreArrivalGuideGuestPayload {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub listing_name: String,
    pub address: String,
    pub date_from: String,
    pub date_to: String,
    pub door_access_code: String,
    pub wifi_ssid: Option<String>,
    pub wifi_password: Option<String>,
    pub check_in_instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostUpcomingArrivalPayload {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub listing_name: String,
    pub guest_name: String,
    pub number_of_persons: i32,
    pub date_from: String,
    pub date_to: String,
    pub door_access_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentHoldExpiryReminderPayload {
    pub booking_id: Uuid,
    pub confirmation_code: String,
    pub listing_name: String,
    pub expires_at: String,
    pub total_price: Decimal,
    pub currency: String,
    pub checkout_url: String,
}

// ============================================================================
// Type State Pattern for Compile-Time Guaranteed Email State Transitions
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessingState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SentState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailedState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailRecord<State> {
    pub id: Uuid,
    pub recipient_email: String,
    pub subject: String,
    pub template_id: String,
    pub payload: serde_json::Value,
    pub attempts: i32,
    pub max_retries: i32,
    pub last_error: Option<String>,
    _state: PhantomData<State>,
}

impl EmailRecord<PendingState> {
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
