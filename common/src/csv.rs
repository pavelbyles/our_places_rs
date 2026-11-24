use crate::payout::PayoutLedgerEntry;

/// Escapes one CSV cell, and neutralises spreadsheet formulas
/// (`=`, `+`, `-`, `@` prefixes) so exported data cannot run in Excel/Sheets.
pub fn csv_cell(value: &str) -> String {
    let guarded = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}

/// The payout ledger as CSV. New columns go at the end, so existing imports keep
/// working.
pub fn format_payout_ledger_csv(entries: &[PayoutLedgerEntry]) -> String {
    let mut csv = String::from(
        "Ledger ID,Booking Confirmation,Property Name,Check-In Date,Check-Out Date,Gross Amount,Currency,Platform Fee Pct,Platform Fee Amount,Tax Withheld,Net Payout Amount,Status,Gateway Reference,Payout Date,Created At,Cleaning Fee,Promo Code,Promo Discount\n",
    );
    for entry in entries {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            entry.id,
            csv_cell(entry.booking_confirmation_code.as_deref().unwrap_or("")),
            csv_cell(entry.listing_name.as_deref().unwrap_or("")),
            entry
                .check_in_date
                .map(|d| d.to_string())
                .unwrap_or_default(),
            entry
                .check_out_date
                .map(|d| d.to_string())
                .unwrap_or_default(),
            entry.gross_amount,
            entry.currency,
            entry.platform_fee_pct,
            entry.platform_fee_amount,
            entry.tax_withheld_amount,
            entry.net_payout_amount,
            entry.status,
            csv_cell(entry.gateway_reference.as_deref().unwrap_or("")),
            entry
                .payout_date
                .map(|d| d.to_rfc3339())
                .unwrap_or_default(),
            entry.created_at.to_rfc3339(),
            entry.cleaning_fee_amount,
            csv_cell(entry.promo_code.as_deref().unwrap_or("")),
            entry.promo_discount_amount,
        ));
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payout::PayoutStatus;
    use chrono::{NaiveDate, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    #[test]
    fn test_format_payout_ledger_csv() {
        let entry = PayoutLedgerEntry {
            id: Uuid::nil(),
            booking_id: Uuid::nil(),
            booking_confirmation_code: Some("ABCDEF12".to_string()),
            listing_id: Uuid::nil(),
            listing_name: Some("Villa \"Ocean View\"".to_string()),
            host_id: Uuid::nil(),
            host_name: Some("John Host".to_string()),
            check_in_date: Some(NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()),
            check_out_date: Some(NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()),
            currency: "USD".to_string(),
            gross_amount: Decimal::new(120000, 2),
            platform_fee_pct: Decimal::new(3, 2),
            platform_fee_amount: Decimal::new(3600, 2),
            tax_withheld_amount: Decimal::ZERO,
            exchange_rate: Decimal::ONE,
            net_payout_amount: Decimal::new(116400, 2),
            cleaning_fee_amount: Decimal::ZERO,
            promo_discount_amount: Decimal::ZERO,
            promo_code: None,
            status: PayoutStatus::Pending,
            gateway_reference: Some("REF123".to_string()),
            failure_reason: None,
            payout_date: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let csv = format_payout_ledger_csv(&[entry]);
        assert!(csv.starts_with("Ledger ID,Booking Confirmation,Property Name"));
        assert!(csv.contains("ABCDEF12"));
        assert!(csv.contains("\"Villa \"\"Ocean View\"\"\""));
        assert!(csv.contains("1200.00"));
        assert!(csv.contains("1164.00"));
        assert!(csv.contains("REF123"));
    }
}

#[cfg(test)]
mod promo_columns {
    use super::*;
    use crate::payout::PayoutStatus;
    use chrono::Utc;
    use rust_decimal::Decimal;
    use uuid::Uuid;

    fn entry(listing_name: &str, promo_code: Option<&str>) -> PayoutLedgerEntry {
        PayoutLedgerEntry {
            id: Uuid::nil(),
            booking_id: Uuid::nil(),
            booking_confirmation_code: Some("ABCDEF12".to_string()),
            listing_id: Uuid::nil(),
            listing_name: Some(listing_name.to_string()),
            host_id: Uuid::nil(),
            host_name: None,
            check_in_date: None,
            check_out_date: None,
            currency: "USD".to_string(),
            gross_amount: Decimal::new(90000, 2),
            platform_fee_pct: Decimal::new(1, 1),
            platform_fee_amount: Decimal::new(9000, 2),
            tax_withheld_amount: Decimal::ZERO,
            exchange_rate: Decimal::ONE,
            net_payout_amount: Decimal::new(81000, 2),
            cleaning_fee_amount: Decimal::new(9000, 2),
            promo_discount_amount: Decimal::new(10000, 2),
            promo_code: promo_code.map(str::to_string),
            status: PayoutStatus::Pending,
            gateway_reference: None,
            failure_reason: None,
            payout_date: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn new_columns_are_added_at_the_end_of_the_header() {
        let csv = format_payout_ledger_csv(&[]);
        assert!(
            csv.starts_with("Ledger ID,Booking Confirmation,Property Name,Check-In Date,Check-Out Date,Gross Amount,Currency,Platform Fee Pct,Platform Fee Amount,Tax Withheld,Net Payout Amount,Status,Gateway Reference,Payout Date,Created At,"),
            "{csv}"
        );
        assert!(
            csv.trim_end()
                .ends_with(",Cleaning Fee,Promo Code,Promo Discount"),
            "{csv}"
        );
    }

    #[test]
    fn rows_carry_the_cleaning_fee_code_and_discount() {
        let csv = format_payout_ledger_csv(&[entry("Villa", Some("SUMMER"))]);
        assert!(
            csv.trim_end().ends_with(",90.00,\"SUMMER\",100.00"),
            "{csv}"
        );
    }

    #[test]
    fn text_cells_are_protected_against_spreadsheet_formulas() {
        for name in ["=HYPERLINK(1)", "+1", "-1", "@cmd"] {
            let csv = format_payout_ledger_csv(&[entry(name, None)]);
            assert!(csv.contains(&format!("\"'{name}\"")), "{csv}");
        }
    }
}
