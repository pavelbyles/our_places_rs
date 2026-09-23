use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PayoutStatus {
    Pending,
    Processing,
    Paid,
    Cancelled,
    Refunded,
}

impl std::fmt::Display for PayoutStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayoutStatus::Pending => write!(f, "pending"),
            PayoutStatus::Processing => write!(f, "processing"),
            PayoutStatus::Paid => write!(f, "paid"),
            PayoutStatus::Cancelled => write!(f, "cancelled"),
            PayoutStatus::Refunded => write!(f, "refunded"),
        }
    }
}

impl std::str::FromStr for PayoutStatus {
    type Err = PayoutTransitionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "pending" => Ok(PayoutStatus::Pending),
            "processing" => Ok(PayoutStatus::Processing),
            "paid" => Ok(PayoutStatus::Paid),
            "cancelled" => Ok(PayoutStatus::Cancelled),
            "refunded" => Ok(PayoutStatus::Refunded),
            _ => Err(PayoutTransitionError::InvalidStatusString(s.to_string())),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PayoutTransitionError {
    #[error("Invalid payout status transition from '{from}' to '{to}'")]
    InvalidTransition {
        from: PayoutStatus,
        to: PayoutStatus,
    },
    #[error("Unknown payout status string: '{0}'")]
    InvalidStatusString(String),
}

impl PayoutStatus {
    pub fn can_transition_to(&self, next: PayoutStatus) -> bool {
        match (self, next) {
            (PayoutStatus::Pending, PayoutStatus::Processing) => true,
            (PayoutStatus::Pending, PayoutStatus::Cancelled) => true,
            (PayoutStatus::Processing, PayoutStatus::Paid) => true,
            (PayoutStatus::Processing, PayoutStatus::Cancelled) => true,
            (PayoutStatus::Paid, PayoutStatus::Refunded) => true,
            (a, b) if *a == b => true,
            _ => false,
        }
    }

    pub fn transition_to(&self, next: PayoutStatus) -> Result<PayoutStatus, PayoutTransitionError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(PayoutTransitionError::InvalidTransition {
                from: *self,
                to: next,
            })
        }
    }
}

// Compile-Time Typestate markers
pub struct PendingPayout;
pub struct ProcessingPayout;
pub struct PaidPayout;
pub struct CancelledPayout;
pub struct RefundedPayout;

pub struct PayoutRecord<State> {
    pub id: Uuid,
    pub booking_id: Uuid,
    pub listing_id: Uuid,
    pub host_id: Uuid,
    pub currency: String,
    pub gross_amount: Decimal,
    pub platform_fee_pct: Decimal,
    pub platform_fee_amount: Decimal,
    pub tax_withheld_amount: Decimal,
    pub exchange_rate: Decimal,
    pub net_payout_amount: Decimal,
    pub gateway_reference: Option<String>,
    pub failure_reason: Option<String>,
    pub payout_date: Option<DateTime<Utc>>,
    pub state: State,
}

impl PayoutRecord<PendingPayout> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        booking_id: Uuid,
        listing_id: Uuid,
        host_id: Uuid,
        currency: String,
        gross_amount: Decimal,
        platform_fee_pct: Decimal,
        platform_fee_amount: Decimal,
        tax_withheld_amount: Decimal,
        exchange_rate: Decimal,
        net_payout_amount: Decimal,
    ) -> Self {
        Self {
            id,
            booking_id,
            listing_id,
            host_id,
            currency,
            gross_amount,
            platform_fee_pct,
            platform_fee_amount,
            tax_withheld_amount,
            exchange_rate,
            net_payout_amount,
            gateway_reference: None,
            failure_reason: None,
            payout_date: None,
            state: PendingPayout,
        }
    }

    pub fn begin_processing(self) -> PayoutRecord<ProcessingPayout> {
        PayoutRecord {
            id: self.id,
            booking_id: self.booking_id,
            listing_id: self.listing_id,
            host_id: self.host_id,
            currency: self.currency,
            gross_amount: self.gross_amount,
            platform_fee_pct: self.platform_fee_pct,
            platform_fee_amount: self.platform_fee_amount,
            tax_withheld_amount: self.tax_withheld_amount,
            exchange_rate: self.exchange_rate,
            net_payout_amount: self.net_payout_amount,
            gateway_reference: self.gateway_reference,
            failure_reason: self.failure_reason,
            payout_date: self.payout_date,
            state: ProcessingPayout,
        }
    }

    pub fn cancel(self, reason: Option<String>) -> PayoutRecord<CancelledPayout> {
        PayoutRecord {
            id: self.id,
            booking_id: self.booking_id,
            listing_id: self.listing_id,
            host_id: self.host_id,
            currency: self.currency,
            gross_amount: self.gross_amount,
            platform_fee_pct: self.platform_fee_pct,
            platform_fee_amount: self.platform_fee_amount,
            tax_withheld_amount: self.tax_withheld_amount,
            exchange_rate: self.exchange_rate,
            net_payout_amount: self.net_payout_amount,
            gateway_reference: self.gateway_reference,
            failure_reason: reason,
            payout_date: self.payout_date,
            state: CancelledPayout,
        }
    }
}

impl PayoutRecord<ProcessingPayout> {
    pub fn mark_paid(
        self,
        payout_date: DateTime<Utc>,
        gateway_reference: Option<String>,
    ) -> PayoutRecord<PaidPayout> {
        PayoutRecord {
            id: self.id,
            booking_id: self.booking_id,
            listing_id: self.listing_id,
            host_id: self.host_id,
            currency: self.currency,
            gross_amount: self.gross_amount,
            platform_fee_pct: self.platform_fee_pct,
            platform_fee_amount: self.platform_fee_amount,
            tax_withheld_amount: self.tax_withheld_amount,
            exchange_rate: self.exchange_rate,
            net_payout_amount: self.net_payout_amount,
            gateway_reference,
            failure_reason: None,
            payout_date: Some(payout_date),
            state: PaidPayout,
        }
    }

    pub fn cancel(self, reason: Option<String>) -> PayoutRecord<CancelledPayout> {
        PayoutRecord {
            id: self.id,
            booking_id: self.booking_id,
            listing_id: self.listing_id,
            host_id: self.host_id,
            currency: self.currency,
            gross_amount: self.gross_amount,
            platform_fee_pct: self.platform_fee_pct,
            platform_fee_amount: self.platform_fee_amount,
            tax_withheld_amount: self.tax_withheld_amount,
            exchange_rate: self.exchange_rate,
            net_payout_amount: self.net_payout_amount,
            gateway_reference: self.gateway_reference,
            failure_reason: reason,
            payout_date: self.payout_date,
            state: CancelledPayout,
        }
    }
}

impl PayoutRecord<PaidPayout> {
    pub fn refund(self, reason: Option<String>) -> PayoutRecord<RefundedPayout> {
        PayoutRecord {
            id: self.id,
            booking_id: self.booking_id,
            listing_id: self.listing_id,
            host_id: self.host_id,
            currency: self.currency,
            gross_amount: self.gross_amount,
            platform_fee_pct: self.platform_fee_pct,
            platform_fee_amount: self.platform_fee_amount,
            tax_withheld_amount: self.tax_withheld_amount,
            exchange_rate: self.exchange_rate,
            net_payout_amount: self.net_payout_amount,
            gateway_reference: self.gateway_reference,
            failure_reason: reason,
            payout_date: self.payout_date,
            state: RefundedPayout,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct PayoutLedgerEntry {
    pub id: Uuid,
    pub booking_id: Uuid,
    pub booking_confirmation_code: Option<String>,
    pub listing_id: Uuid,
    pub listing_name: Option<String>,
    pub host_id: Uuid,
    pub host_name: Option<String>,
    pub check_in_date: Option<NaiveDate>,
    pub check_out_date: Option<NaiveDate>,
    pub currency: String,
    pub gross_amount: Decimal,
    pub platform_fee_pct: Decimal,
    pub platform_fee_amount: Decimal,
    pub tax_withheld_amount: Decimal,
    pub exchange_rate: Decimal,
    pub net_payout_amount: Decimal,
    pub status: PayoutStatus,
    pub gateway_reference: Option<String>,
    pub failure_reason: Option<String>,
    pub payout_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, Default)]
pub struct PayoutSummary {
    pub total_gross: Decimal,
    pub total_platform_fee: Decimal,
    pub total_tax_withheld: Decimal,
    pub total_net: Decimal,
    pub total_paid: Decimal,
    pub total_pending: Decimal,
    pub total_processing: Decimal,
    pub count_entries: i64,
}

#[derive(Debug, Deserialize, Clone, IntoParams, Default)]
pub struct PayoutFilter {
    pub listing_id: Option<Uuid>,
    pub host_id: Option<Uuid>,
    pub status: Option<PayoutStatus>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct UpdatePayoutStatusRequest {
    pub status: PayoutStatus,
    pub gateway_reference: Option<String>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct PayoutLedgerResponse {
    pub entries: Vec<PayoutLedgerEntry>,
    pub total_count: i64,
    pub page: u32,
    pub per_page: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_payout_status_transitions() {
        let status = PayoutStatus::Pending;
        assert_eq!(
            status.transition_to(PayoutStatus::Processing).unwrap(),
            PayoutStatus::Processing
        );
        assert_eq!(
            status.transition_to(PayoutStatus::Cancelled).unwrap(),
            PayoutStatus::Cancelled
        );

        let processing = PayoutStatus::Processing;
        assert_eq!(
            processing.transition_to(PayoutStatus::Paid).unwrap(),
            PayoutStatus::Paid
        );
        assert_eq!(
            processing.transition_to(PayoutStatus::Cancelled).unwrap(),
            PayoutStatus::Cancelled
        );

        let paid = PayoutStatus::Paid;
        assert_eq!(
            paid.transition_to(PayoutStatus::Refunded).unwrap(),
            PayoutStatus::Refunded
        );
    }

    #[test]
    fn test_invalid_payout_status_transitions() {
        let paid = PayoutStatus::Paid;
        assert!(paid.transition_to(PayoutStatus::Pending).is_err());
        assert!(paid.transition_to(PayoutStatus::Processing).is_err());
        assert!(paid.transition_to(PayoutStatus::Cancelled).is_err());

        let cancelled = PayoutStatus::Cancelled;
        assert!(cancelled.transition_to(PayoutStatus::Pending).is_err());
        assert!(cancelled.transition_to(PayoutStatus::Paid).is_err());

        let refunded = PayoutStatus::Refunded;
        assert!(refunded.transition_to(PayoutStatus::Paid).is_err());
    }

    #[test]
    fn test_typestate_payout_record() {
        let record = PayoutRecord::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "USD".to_string(),
            Decimal::new(1000, 0),
            Decimal::new(3, 2),
            Decimal::new(30, 0),
            Decimal::ZERO,
            Decimal::ONE,
            Decimal::new(970, 0),
        );

        let processing = record.begin_processing();
        let paid = processing.mark_paid(Utc::now(), Some("MPGS_12345".to_string()));
        assert_eq!(paid.gateway_reference, Some("MPGS_12345".to_string()));

        let refunded = paid.refund(Some("Chargeback requested".to_string()));
        assert_eq!(
            refunded.failure_reason,
            Some("Chargeback requested".to_string())
        );
    }
}
