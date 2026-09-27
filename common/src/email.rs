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
    GuestHostMessageNotification,
}

impl EmailTemplate {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserVerificationOtp => "user_verification_otp",
            Self::PasswordResetOtp => "password_reset_otp",
            Self::BookingConfirmation => "booking_confirmation",
            Self::GuestHostMessageNotification => "guest_host_message_notification",
        }
    }
}

impl fmt::Display for EmailTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
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
        assert_eq!(
            EmailTemplate::UserVerificationOtp.as_str(),
            "user_verification_otp"
        );
        assert_eq!(
            EmailTemplate::PasswordResetOtp.as_str(),
            "password_reset_otp"
        );
        assert_eq!(
            EmailTemplate::BookingConfirmation.as_str(),
            "booking_confirmation"
        );
        assert_eq!(
            EmailTemplate::GuestHostMessageNotification.as_str(),
            "guest_host_message_notification"
        );
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
