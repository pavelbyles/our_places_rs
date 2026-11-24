use std::fmt;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Type of tax compliance document.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "tax_document_type", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum TaxDocumentType {
    /// Initial statutory payment receipt issued upon successful payment.
    PaymentReceipt,
    /// Formal statutory tax invoice issued after stay check-in / completion.
    TaxInvoice,
    /// Statutory refund receipt issued upon booking cancellation and refund.
    RefundReceipt,
}

impl fmt::Display for TaxDocumentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PaymentReceipt => write!(f, "payment_receipt"),
            Self::TaxInvoice => write!(f, "tax_invoice"),
            Self::RefundReceipt => write!(f, "refund_receipt"),
        }
    }
}

impl std::str::FromStr for TaxDocumentType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "payment_receipt" => Ok(Self::PaymentReceipt),
            "tax_invoice" => Ok(Self::TaxInvoice),
            "refund_receipt" => Ok(Self::RefundReceipt),
            other => Err(format!("Unknown tax document type: {other}")),
        }
    }
}

/// Lifecycle status of an issued tax document.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "tax_document_status", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum TaxDocumentStatus {
    /// Active, legally valid document version.
    Issued,
    /// Superseded by a subsequent revision or cancellation credit note.
    Superseded,
}

impl fmt::Display for TaxDocumentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Issued => write!(f, "issued"),
            Self::Superseded => write!(f, "superseded"),
        }
    }
}

impl std::str::FromStr for TaxDocumentStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "issued" => Ok(Self::Issued),
            "superseded" => Ok(Self::Superseded),
            other => Err(format!("Unknown tax document status: {other}")),
        }
    }
}

/// Tax classification for an invoice line item.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceTaxCategory {
    /// Standard statutory General Consumption Tax rate (15%).
    Gct,
    /// Non-taxable or exempt charge (e.g. security deposit, platform fee).
    Exempt,
}

/// An itemized line item on a tax document.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq, Eq)]
pub struct InvoiceLineItem {
    /// What the line is for.
    pub description: String,
    /// How many units.
    pub quantity: u32,
    /// Price per unit.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "150.00")]
    pub unit_price: Decimal,
    /// Tax treatment of the line.
    pub tax_category: InvoiceTaxCategory,
    /// Tax rate as a fraction.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "0.1500")]
    pub tax_rate: Decimal,
    /// Tax on the line.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "22.50")]
    pub tax_amount: Decimal,
    /// Line total, including tax.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "172.50")]
    pub total_amount: Decimal,
}

/// DTO representing an issued compliance document.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct TaxDocumentResponse {
    /// Unique identifier.
    pub id: Uuid,
    /// Kind of document.
    pub document_type: TaxDocumentType,
    /// Human-readable document number.
    pub document_number: String,
    /// Revision number of this document.
    pub revision: i32,
    /// Whether the document is current.
    pub status: TaxDocumentStatus,
    /// The document this one replaces, if any.
    pub supersedes_id: Option<Uuid>,
    /// The booking this belongs to.
    pub booking_id: Uuid,
    /// The booking history snapshot this was issued from, if any.
    pub booking_history_id: Option<Uuid>,
    /// The user this was issued to.
    pub recipient_id: Uuid,
    /// The listing this belongs to.
    pub listing_id: Uuid,
    /// When this was issued.
    pub issue_date: DateTime<Utc>,
    /// Legal name of the supplier.
    pub supplier_name: String,
    /// Registered address of the supplier.
    pub supplier_address: String,
    /// Tax registration number (TRN) of the supplier.
    pub supplier_trn: String,
    /// Name of the recipient.
    pub recipient_name: String,
    /// Email address of the recipient.
    pub recipient_email: String,
    /// Company of the recipient, for business stays.
    pub recipient_company_name: Option<String>,
    /// Tax registration number (TRN) of the recipient, for business stays.
    pub recipient_trn: Option<String>,
    /// Currency of the amounts.
    pub currency: String,
    /// Listing currency to payment currency, locked on the booking.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "1.000000")]
    pub exchange_rate: Decimal,
    /// Document currency to Jamaican dollars, effective at the issue date.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "158.500000")]
    pub jmd_exchange_rate: Decimal,
    /// Accommodation after discounts.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "500.00")]
    pub accommodation_net: Decimal,
    /// One-time cleaning fee.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "50.00")]
    pub cleaning_fee: Decimal,
    /// Amount tax is charged on.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "550.00")]
    pub taxable_amount: Decimal,
    /// Tax rate as a fraction, such as `0.15` for 15%.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "0.1500")]
    pub tax_rate: Decimal,
    /// Tax charged.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "82.50")]
    pub tax_amount: Decimal,
    /// Guest platform fee.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "25.00")]
    pub platform_fee: Decimal,
    /// Refundable damage deposit.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "200.00")]
    pub damage_deposit: Decimal,
    /// Total charged.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "857.50")]
    pub total_amount: Decimal,
    /// Amount already paid, on a revised document.
    #[serde(default, with = "rust_decimal::serde::str_option")]
    #[schema(value_type = Option<String>, example = "857.50")]
    pub amount_previously_paid: Option<Decimal>,
    /// Amount still owed, on a revised document.
    #[serde(default, with = "rust_decimal::serde::str_option")]
    #[schema(value_type = Option<String>, example = "0.00")]
    pub balance_due: Option<Decimal>,
    /// Refund percentage, on a cancellation document.
    #[serde(default, with = "rust_decimal::serde::str_option")]
    #[schema(value_type = Option<String>, example = "1.0000")]
    pub refund_pct: Option<Decimal>,
    /// Amount kept after a cancellation.
    #[serde(default, with = "rust_decimal::serde::str_option")]
    #[schema(value_type = Option<String>, example = "0.00")]
    pub retained_amount: Option<Decimal>,
    /// Tax charged, in Jamaican dollars.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "13076.25")]
    pub jmd_tax_amount: Decimal,
    /// Total charged, in Jamaican dollars.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "135913.75")]
    pub jmd_total_amount: Decimal,
    /// Invoice line items.
    pub line_items: Vec<InvoiceLineItem>,
    /// The code the booking redeemed, if any.
    #[serde(default)]
    pub promo_code: Option<String>,
    /// The whole promo discount. Already reflected in the net amounts.
    #[serde(default, with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "100.00")]
    pub promo_discount: Decimal,
    /// When this was created.
    pub created_at: DateTime<Utc>,
}

/// Generates a standardized document number.
/// - Payment Receipt: `REC-YYYY-{code}` for revision 1; `REC-YYYY-{code}-R{revision}` for revisions >= 2
/// - Tax Invoice: `INV-YYYY-{code}`
/// - Refund Receipt: `REF-YYYY-{code}`
pub fn document_number(
    doc_type: TaxDocumentType,
    year: i32,
    confirmation_code: &str,
    revision: i32,
) -> String {
    match doc_type {
        TaxDocumentType::PaymentReceipt => {
            if revision <= 1 {
                format!("REC-{year}-{confirmation_code}")
            } else {
                format!("REC-{year}-{confirmation_code}-R{revision}")
            }
        }
        TaxDocumentType::TaxInvoice => format!("INV-{year}-{confirmation_code}"),
        TaxDocumentType::RefundReceipt => format!("REF-{year}-{confirmation_code}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_number_formatting() {
        assert_eq!(
            document_number(TaxDocumentType::PaymentReceipt, 2026, "JMK92Z", 1),
            "REC-2026-JMK92Z"
        );
        assert_eq!(
            document_number(TaxDocumentType::PaymentReceipt, 2026, "JMK92Z", 2),
            "REC-2026-JMK92Z-R2"
        );
        assert_eq!(
            document_number(TaxDocumentType::PaymentReceipt, 2026, "JMK92Z", 3),
            "REC-2026-JMK92Z-R3"
        );
        assert_eq!(
            document_number(TaxDocumentType::TaxInvoice, 2026, "JMK92Z", 1),
            "INV-2026-JMK92Z"
        );
        assert_eq!(
            document_number(TaxDocumentType::RefundReceipt, 2026, "JMK92Z", 1),
            "REF-2026-JMK92Z"
        );
    }

    #[test]
    fn test_tax_document_type_and_status_display_parsing_serde() {
        use std::str::FromStr;

        assert_eq!(
            TaxDocumentType::PaymentReceipt.to_string(),
            "payment_receipt"
        );
        assert_eq!(TaxDocumentType::TaxInvoice.to_string(), "tax_invoice");
        assert_eq!(TaxDocumentType::RefundReceipt.to_string(), "refund_receipt");

        assert_eq!(
            TaxDocumentType::from_str("payment_receipt").unwrap(),
            TaxDocumentType::PaymentReceipt
        );
        assert_eq!(
            TaxDocumentType::from_str("Tax_Invoice").unwrap(),
            TaxDocumentType::TaxInvoice
        );
        assert_eq!(
            TaxDocumentType::from_str("REFUND_RECEIPT").unwrap(),
            TaxDocumentType::RefundReceipt
        );
        assert!(TaxDocumentType::from_str("unknown").is_err());

        assert_eq!(TaxDocumentStatus::Issued.to_string(), "issued");
        assert_eq!(TaxDocumentStatus::Superseded.to_string(), "superseded");

        assert_eq!(
            TaxDocumentStatus::from_str("issued").unwrap(),
            TaxDocumentStatus::Issued
        );
        assert_eq!(
            TaxDocumentStatus::from_str("SUPERSEDED").unwrap(),
            TaxDocumentStatus::Superseded
        );
        assert!(TaxDocumentStatus::from_str("unknown").is_err());

        let json = serde_json::to_string(&TaxDocumentType::TaxInvoice).unwrap();
        assert_eq!(json, "\"tax_invoice\"");
        let parsed: TaxDocumentType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, TaxDocumentType::TaxInvoice);

        let json_status = serde_json::to_string(&TaxDocumentStatus::Issued).unwrap();
        assert_eq!(json_status, "\"issued\"");
        let parsed_status: TaxDocumentStatus = serde_json::from_str(&json_status).unwrap();
        assert_eq!(parsed_status, TaxDocumentStatus::Issued);
    }
}
