use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Payout lifecycle status for host disbursement records.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "payout_status", rename_all = "lowercase")
)]
#[serde(rename_all = "lowercase")]
pub enum PayoutStatus {
    /// Payout ledger entry created and pending processing schedule.
    Pending,
    /// Payout batch is currently being processed with payment provider.
    Processing,
    /// Payout has been successfully remitted to host payout destination.
    Paid,
    /// Payout was cancelled prior to funds disbursement.
    Cancelled,
    /// Payout was refunded or reversed post-settlement.
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

/// Why a payout status change was refused.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PayoutTransitionError {
    /// The status can't move from `from` to `to`.
    #[error("Invalid payout status transition from '{from}' to '{to}'")]
    InvalidTransition {
        /// The current status.
        from: PayoutStatus,
        /// The status asked for.
        to: PayoutStatus,
    },
    /// The text isn't a known status.
    #[error("Unknown payout status string: '{0}'")]
    InvalidStatusString(String),
}

impl PayoutStatus {
    /// Whether the status may move to `next`.
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

    /// Moves to `next`, or says why it can't.
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
/// Typestate: waiting to be sent.
pub struct PendingPayout;
/// Typestate: being sent.
pub struct ProcessingPayout;
/// Typestate: paid to the host.
pub struct PaidPayout;
/// Typestate: cancelled before payment.
pub struct CancelledPayout;
/// Typestate: paid, then returned.
pub struct RefundedPayout;

/// A payout whose stage is checked at compile time.
pub struct PayoutRecord<State> {
    /// Unique identifier.
    pub id: Uuid,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// The host.
    pub host_id: Uuid,
    /// Currency of the amounts.
    pub currency: String,
    /// Host's gross for the booking, before commission.
    pub gross_amount: Decimal,
    /// Commission rate applied.
    pub platform_fee_pct: Decimal,
    /// Commission taken.
    pub platform_fee_amount: Decimal,
    /// Tax withheld from the payout.
    pub tax_withheld_amount: Decimal,
    /// Booking currency to host currency, effective at confirmation.
    pub exchange_rate: Decimal,
    /// What the host receives.
    pub net_payout_amount: Decimal,
    /// Part of the gross that is the cleaning fee, in this row's currency.
    pub cleaning_fee_amount: Decimal,
    /// The promo discount the host funded, in this row's currency.
    pub promo_discount_amount: Decimal,
    /// Payment gateway reference, once sent.
    pub gateway_reference: Option<String>,
    /// Why the payout failed or was cancelled, if it did.
    pub failure_reason: Option<String>,
    /// When the payout was made, if it has been.
    pub payout_date: Option<DateTime<Utc>>,
    /// Which stage the payout is at, enforced at compile time.
    pub state: State,
}

/// Everything needed to open a pending payout. Named fields keep the six
/// money values from being passed in the wrong order.
#[derive(Debug, Clone)]
pub struct NewPayout {
    /// Unique identifier.
    pub id: Uuid,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// The host.
    pub host_id: Uuid,
    /// Currency of the amounts.
    pub currency: String,
    /// Host's gross for the booking, before commission.
    pub gross_amount: Decimal,
    /// Commission rate applied.
    pub platform_fee_pct: Decimal,
    /// Commission taken.
    pub platform_fee_amount: Decimal,
    /// Tax withheld from the payout.
    pub tax_withheld_amount: Decimal,
    /// Booking currency to host currency, effective at confirmation.
    pub exchange_rate: Decimal,
    /// What the host receives.
    pub net_payout_amount: Decimal,
    /// Part of the gross that is the cleaning fee, in this row's currency.
    pub cleaning_fee_amount: Decimal,
    /// The promo discount the host funded, in this row's currency.
    pub promo_discount_amount: Decimal,
}

impl PayoutRecord<PendingPayout> {
    /// A new payout, waiting to be sent.
    pub fn new(payout: NewPayout) -> Self {
        Self {
            id: payout.id,
            booking_id: payout.booking_id,
            listing_id: payout.listing_id,
            host_id: payout.host_id,
            currency: payout.currency,
            gross_amount: payout.gross_amount,
            platform_fee_pct: payout.platform_fee_pct,
            platform_fee_amount: payout.platform_fee_amount,
            tax_withheld_amount: payout.tax_withheld_amount,
            exchange_rate: payout.exchange_rate,
            net_payout_amount: payout.net_payout_amount,
            cleaning_fee_amount: payout.cleaning_fee_amount,
            promo_discount_amount: payout.promo_discount_amount,
            gateway_reference: None,
            failure_reason: None,
            payout_date: None,
            state: PendingPayout,
        }
    }

    /// Starts sending the payout.
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
            cleaning_fee_amount: self.cleaning_fee_amount,
            promo_discount_amount: self.promo_discount_amount,
            gateway_reference: self.gateway_reference,
            failure_reason: self.failure_reason,
            payout_date: self.payout_date,
            state: ProcessingPayout,
        }
    }

    /// Cancels the payout, with the reason.
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
            cleaning_fee_amount: self.cleaning_fee_amount,
            promo_discount_amount: self.promo_discount_amount,
            gateway_reference: self.gateway_reference,
            failure_reason: reason,
            payout_date: self.payout_date,
            state: CancelledPayout,
        }
    }
}

impl PayoutRecord<ProcessingPayout> {
    /// Records that the payout was paid.
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
            cleaning_fee_amount: self.cleaning_fee_amount,
            promo_discount_amount: self.promo_discount_amount,
            gateway_reference,
            failure_reason: None,
            payout_date: Some(payout_date),
            state: PaidPayout,
        }
    }

    /// Cancels the payout, with the reason.
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
            cleaning_fee_amount: self.cleaning_fee_amount,
            promo_discount_amount: self.promo_discount_amount,
            gateway_reference: self.gateway_reference,
            failure_reason: reason,
            payout_date: self.payout_date,
            state: CancelledPayout,
        }
    }
}

impl PayoutRecord<PaidPayout> {
    /// Records that a paid payout was returned, with the reason.
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
            cleaning_fee_amount: self.cleaning_fee_amount,
            promo_discount_amount: self.promo_discount_amount,
            gateway_reference: self.gateway_reference,
            failure_reason: reason,
            payout_date: self.payout_date,
            state: RefundedPayout,
        }
    }
}

/// A ledger row with its booking and listing details.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct PayoutLedgerEntry {
    /// Unique identifier.
    pub id: Uuid,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// Confirmation code of the booking.
    pub booking_confirmation_code: Option<String>,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// Name of the listing.
    pub listing_name: Option<String>,
    /// The host.
    pub host_id: Uuid,
    /// Name of the host.
    pub host_name: Option<String>,
    /// First night of the stay.
    pub check_in_date: Option<NaiveDate>,
    /// Departure date of the stay.
    pub check_out_date: Option<NaiveDate>,
    /// Currency of the amounts.
    pub currency: String,
    /// Host's gross for the booking, before commission.
    pub gross_amount: Decimal,
    /// Commission rate applied.
    pub platform_fee_pct: Decimal,
    /// Commission taken.
    pub platform_fee_amount: Decimal,
    /// Tax withheld from the payout.
    pub tax_withheld_amount: Decimal,
    /// Booking currency to host currency, effective at confirmation.
    pub exchange_rate: Decimal,
    /// What the host receives.
    pub net_payout_amount: Decimal,
    /// Part of the gross that is the cleaning fee, in this row's currency.
    #[serde(default)]
    pub cleaning_fee_amount: Decimal,
    /// The promo discount the host funded, in this row's currency.
    #[serde(default)]
    pub promo_discount_amount: Decimal,
    /// The promo code the booking redeemed, if any.
    #[serde(default)]
    pub promo_code: Option<String>,
    /// Payout status.
    pub status: PayoutStatus,
    /// Payment gateway reference, once sent.
    pub gateway_reference: Option<String>,
    /// Why the payout failed or was cancelled, if it did.
    pub failure_reason: Option<String>,
    /// When the payout was made, if it has been.
    pub payout_date: Option<DateTime<Utc>>,
    /// When this was created.
    pub created_at: DateTime<Utc>,
    /// When this was last changed.
    pub updated_at: DateTime<Utc>,
}

/// Ledger totals.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, Default)]
pub struct PayoutSummary {
    /// Sum of gross amounts.
    pub total_gross: Decimal,
    /// Sum of commission taken.
    pub total_platform_fee: Decimal,
    /// Sum of tax withheld.
    pub total_tax_withheld: Decimal,
    /// Sum of net payouts.
    pub total_net: Decimal,
    /// Sum of net payouts already paid.
    pub total_paid: Decimal,
    /// Sum of net payouts still pending.
    pub total_pending: Decimal,
    /// Sum of net payouts being processed.
    pub total_processing: Decimal,
    /// Number of ledger entries.
    pub count_entries: i64,
}

/// What to include from the ledger.
#[derive(Debug, Deserialize, Clone, IntoParams, Default)]
pub struct PayoutFilter {
    /// Only this listing.
    pub listing_id: Option<Uuid>,
    /// Only this host.
    pub host_id: Option<Uuid>,
    /// Only this status.
    pub status: Option<PayoutStatus>,
    /// Earliest date to include.
    pub date_from: Option<NaiveDate>,
    /// Latest date to include.
    pub date_to: Option<NaiveDate>,
    /// Page number, starting at 1.
    pub page: Option<u32>,
    /// Results per page.
    pub per_page: Option<u32>,
}

/// Request to change a payout's status.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct UpdatePayoutStatusRequest {
    /// The status to move to.
    pub status: PayoutStatus,
    /// Payment gateway reference, once sent.
    pub gateway_reference: Option<String>,
    /// Updated why the payout failed or was cancelled, if it did.
    pub failure_reason: Option<String>,
}

/// A page of the ledger.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct PayoutLedgerResponse {
    /// The ledger entries on this page.
    pub entries: Vec<PayoutLedgerEntry>,
    /// Entries across all pages.
    pub total_count: i64,
    /// This page's number.
    pub page: u32,
    /// Entries per page.
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
        let record = PayoutRecord::new(NewPayout {
            id: Uuid::new_v4(),
            booking_id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            host_id: Uuid::new_v4(),
            currency: "USD".to_string(),
            gross_amount: Decimal::new(1000, 0),
            platform_fee_pct: Decimal::new(3, 2),
            platform_fee_amount: Decimal::new(30, 0),
            tax_withheld_amount: Decimal::ZERO,
            exchange_rate: Decimal::ONE,
            net_payout_amount: Decimal::new(970, 0),
            cleaning_fee_amount: Decimal::ZERO,
            promo_discount_amount: Decimal::ZERO,
        });

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
