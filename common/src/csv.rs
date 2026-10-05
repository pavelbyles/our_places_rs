use crate::payout::PayoutLedgerEntry;

pub fn format_payout_ledger_csv(entries: &[PayoutLedgerEntry]) -> String {
    let mut csv = String::from(
        "Ledger ID,Booking Confirmation,Property Name,Check-In Date,Check-Out Date,Gross Amount,Currency,Platform Fee Pct,Platform Fee Amount,Tax Withheld,Net Payout Amount,Status,Gateway Reference,Payout Date,Created At\n",
    );
    for entry in entries {
        csv.push_str(&format!(
            "{},{},\"{}\",{},{},{},{},{},{},{},{},{},\"{}\",{},{}\n",
            entry.id,
            entry.booking_confirmation_code.as_deref().unwrap_or(""),
            entry
                .listing_name
                .as_deref()
                .unwrap_or("")
                .replace('"', "\"\""),
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
            entry
                .gateway_reference
                .as_deref()
                .unwrap_or("")
                .replace('"', "\"\""),
            entry
                .payout_date
                .map(|d| d.to_rfc3339())
                .unwrap_or_default(),
            entry.created_at.to_rfc3339(),
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
