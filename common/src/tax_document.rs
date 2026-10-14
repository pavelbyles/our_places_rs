use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Type of tax compliance document.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaxDocumentType {
    PaymentReceipt,
    TaxInvoice,
    RefundReceipt,
}

/// Lifecycle status of an issued tax document.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaxDocumentStatus {
    Issued,
    Superseded,
}

/// Tax classification for an invoice line item.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceTaxCategory {
    Gct,
    Exempt,
}

/// An itemized line item on a tax document.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq, Eq)]
pub struct InvoiceLineItem {
    pub description: String,
    pub quantity: u32,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "150.00")]
    pub unit_price: Decimal,
    pub tax_category: InvoiceTaxCategory,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "0.1500")]
    pub tax_rate: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "22.50")]
    pub tax_amount: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "172.50")]
    pub total_amount: Decimal,
}

/// DTO representing an issued compliance document.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct TaxDocumentResponse {
    pub id: Uuid,
    pub document_type: TaxDocumentType,
    pub document_number: String,
    pub revision: i32,
    pub status: TaxDocumentStatus,
    pub supersedes_id: Option<Uuid>,
    pub booking_id: Uuid,
    pub booking_history_id: Option<Uuid>,
    pub recipient_id: Uuid,
    pub listing_id: Uuid,
    pub issue_date: DateTime<Utc>,
    pub supplier_name: String,
    pub supplier_address: String,
    pub supplier_trn: String,
    pub recipient_name: String,
    pub recipient_email: String,
    pub recipient_company_name: Option<String>,
    pub recipient_trn: Option<String>,
    pub currency: String,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "1.000000")]
    pub exchange_rate: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "158.500000")]
    pub jmd_exchange_rate: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "500.00")]
    pub accommodation_net: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "50.00")]
    pub cleaning_fee: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "550.00")]
    pub taxable_amount: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "0.1500")]
    pub tax_rate: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "82.50")]
    pub tax_amount: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "25.00")]
    pub platform_fee: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "200.00")]
    pub damage_deposit: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "857.50")]
    pub total_amount: Decimal,
    #[serde(default, with = "rust_decimal::serde::str_option")]
    #[schema(value_type = Option<String>, example = "857.50")]
    pub amount_previously_paid: Option<Decimal>,
    #[serde(default, with = "rust_decimal::serde::str_option")]
    #[schema(value_type = Option<String>, example = "0.00")]
    pub balance_due: Option<Decimal>,
    #[serde(default, with = "rust_decimal::serde::str_option")]
    #[schema(value_type = Option<String>, example = "1.0000")]
    pub refund_pct: Option<Decimal>,
    #[serde(default, with = "rust_decimal::serde::str_option")]
    #[schema(value_type = Option<String>, example = "0.00")]
    pub retained_amount: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "13076.25")]
    pub jmd_tax_amount: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "135913.75")]
    pub jmd_total_amount: Decimal,
    pub line_items: Vec<InvoiceLineItem>,
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
}
