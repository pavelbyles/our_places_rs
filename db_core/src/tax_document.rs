//! Statutory tax documents and compliance invoicing.

use chrono::{Datelike, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{
    Booking, NewTaxDocument, SupplierInfo, TaxDocumentRow, TaxDocumentStatus, TaxDocumentType,
};

/// Inserts a new statutory tax document into the database.
pub async fn insert_tax_document(
    conn: &mut PgConnection,
    doc: &NewTaxDocument,
) -> Result<TaxDocumentRow> {
    let row = sqlx::query_as!(
        TaxDocumentRow,
        r#"
        INSERT INTO tax_document (
            document_type,
            document_number,
            revision,
            status,
            supersedes_id,
            booking_id,
            booking_history_id,
            recipient_id,
            listing_id,
            issue_date,
            supplier_name,
            supplier_address,
            supplier_trn,
            recipient_name,
            recipient_email,
            recipient_company_name,
            recipient_trn,
            currency,
            exchange_rate,
            jmd_exchange_rate,
            accommodation_net,
            cleaning_fee,
            taxable_amount,
            tax_rate,
            tax_amount,
            platform_fee,
            damage_deposit,
            total_amount,
            amount_previously_paid,
            balance_due,
            refund_pct,
            retained_amount,
            jmd_tax_amount,
            jmd_total_amount,
            line_items
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
            $21, $22, $23, $24, $25, $26, $27, $28, $29, $30,
            $31, $32, $33, $34, $35
        )
        RETURNING
            id,
            document_type as "document_type: TaxDocumentType",
            document_number,
            revision,
            status as "status: TaxDocumentStatus",
            supersedes_id,
            booking_id,
            booking_history_id,
            recipient_id,
            listing_id,
            issue_date,
            supplier_name,
            supplier_address,
            supplier_trn,
            recipient_name,
            recipient_email,
            recipient_company_name,
            recipient_trn,
            currency,
            exchange_rate,
            jmd_exchange_rate,
            accommodation_net,
            cleaning_fee,
            taxable_amount,
            tax_rate,
            tax_amount,
            platform_fee,
            damage_deposit,
            total_amount,
            amount_previously_paid,
            balance_due,
            refund_pct,
            retained_amount,
            jmd_tax_amount,
            jmd_total_amount,
            line_items,
            created_at
        "#,
        doc.document_type as TaxDocumentType,
        doc.document_number,
        doc.revision,
        doc.status as TaxDocumentStatus,
        doc.supersedes_id,
        doc.booking_id,
        doc.booking_history_id,
        doc.recipient_id,
        doc.listing_id,
        doc.issue_date,
        doc.supplier_name,
        doc.supplier_address,
        doc.supplier_trn,
        doc.recipient_name,
        doc.recipient_email,
        doc.recipient_company_name,
        doc.recipient_trn,
        doc.currency,
        doc.exchange_rate,
        doc.jmd_exchange_rate,
        doc.accommodation_net,
        doc.cleaning_fee,
        doc.taxable_amount,
        doc.tax_rate,
        doc.tax_amount,
        doc.platform_fee,
        doc.damage_deposit,
        doc.total_amount,
        doc.amount_previously_paid,
        doc.balance_due,
        doc.refund_pct,
        doc.retained_amount,
        doc.jmd_tax_amount,
        doc.jmd_total_amount,
        doc.line_items
    )
    .fetch_one(&mut *conn)
    .await?;

    Ok(row)
}

/// Supersedes any active (`status = 'issued'`) document of the specified type for a booking.
pub async fn supersede_active(
    conn: &mut PgConnection,
    booking_id: Uuid,
    doc_type: TaxDocumentType,
) -> Result<Option<TaxDocumentRow>> {
    let row = sqlx::query_as!(
        TaxDocumentRow,
        r#"
        UPDATE tax_document
        SET status = 'superseded'::tax_document_status
        WHERE booking_id = $1 AND document_type = $2 AND status = 'issued'::tax_document_status
        RETURNING
            id,
            document_type as "document_type: TaxDocumentType",
            document_number,
            revision,
            status as "status: TaxDocumentStatus",
            supersedes_id,
            booking_id,
            booking_history_id,
            recipient_id,
            listing_id,
            issue_date,
            supplier_name,
            supplier_address,
            supplier_trn,
            recipient_name,
            recipient_email,
            recipient_company_name,
            recipient_trn,
            currency,
            exchange_rate,
            jmd_exchange_rate,
            accommodation_net,
            cleaning_fee,
            taxable_amount,
            tax_rate,
            tax_amount,
            platform_fee,
            damage_deposit,
            total_amount,
            amount_previously_paid,
            balance_due,
            refund_pct,
            retained_amount,
            jmd_tax_amount,
            jmd_total_amount,
            line_items,
            created_at
        "#,
        booking_id,
        doc_type as TaxDocumentType
    )
    .fetch_optional(&mut *conn)
    .await?;

    Ok(row)
}

/// Lists all statutory tax documents associated with a booking.
pub async fn list_for_booking<'e, E>(executor: E, booking_id: Uuid) -> Result<Vec<TaxDocumentRow>>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as!(
        TaxDocumentRow,
        r#"
        SELECT
            id,
            document_type as "document_type: TaxDocumentType",
            document_number,
            revision,
            status as "status: TaxDocumentStatus",
            supersedes_id,
            booking_id,
            booking_history_id,
            recipient_id,
            listing_id,
            issue_date,
            supplier_name,
            supplier_address,
            supplier_trn,
            recipient_name,
            recipient_email,
            recipient_company_name,
            recipient_trn,
            currency,
            exchange_rate,
            jmd_exchange_rate,
            accommodation_net,
            cleaning_fee,
            taxable_amount,
            tax_rate,
            tax_amount,
            platform_fee,
            damage_deposit,
            total_amount,
            amount_previously_paid,
            balance_due,
            refund_pct,
            retained_amount,
            jmd_tax_amount,
            jmd_total_amount,
            line_items,
            created_at
        FROM tax_document
        WHERE booking_id = $1
        ORDER BY created_at DESC, revision DESC
        "#,
        booking_id
    )
    .fetch_all(executor)
    .await?;

    Ok(rows)
}

/// Retrieves a specific tax document by booking identifier and document number.
pub async fn get_by_number<'e, E>(
    executor: E,
    booking_id: Uuid,
    document_number: &str,
) -> Result<Option<TaxDocumentRow>>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query_as!(
        TaxDocumentRow,
        r#"
        SELECT
            id,
            document_type as "document_type: TaxDocumentType",
            document_number,
            revision,
            status as "status: TaxDocumentStatus",
            supersedes_id,
            booking_id,
            booking_history_id,
            recipient_id,
            listing_id,
            issue_date,
            supplier_name,
            supplier_address,
            supplier_trn,
            recipient_name,
            recipient_email,
            recipient_company_name,
            recipient_trn,
            currency,
            exchange_rate,
            jmd_exchange_rate,
            accommodation_net,
            cleaning_fee,
            taxable_amount,
            tax_rate,
            tax_amount,
            platform_fee,
            damage_deposit,
            total_amount,
            amount_previously_paid,
            balance_due,
            refund_pct,
            retained_amount,
            jmd_tax_amount,
            jmd_total_amount,
            line_items,
            created_at
        FROM tax_document
        WHERE booking_id = $1 AND document_number = $2
        "#,
        booking_id,
        document_number
    )
    .fetch_optional(executor)
    .await?;

    Ok(row)
}

/// Issues a payment receipt document upon booking confirmation.
pub async fn issue_payment_receipt(
    conn: &mut PgConnection,
    booking: &Booking,
    supplier: &SupplierInfo,
    booking_history_id: Option<Uuid>,
) -> Result<TaxDocumentRow> {
    let issue_date = Utc::now();

    // Look up JMD FX rate
    let jmd_exchange_rate =
        crate::exchange_rate::get_rate(&mut *conn, &booking.currency, "JMD", issue_date).await?;

    // Determine revision number and supersede active if necessary
    let prev_max_rev = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(MAX(revision), 0)
        FROM tax_document
        WHERE booking_id = $1 AND document_type = 'payment_receipt'::tax_document_type
        "#,
        booking.id
    )
    .fetch_one(&mut *conn)
    .await?
    .unwrap_or(0);

    let revision = prev_max_rev + 1;

    let (supersedes_id, amount_previously_paid, balance_due) = if revision > 1 {
        let superseded =
            supersede_active(conn, booking.id, TaxDocumentType::PaymentReceipt).await?;
        let sid = superseded.as_ref().map(|d| d.id);
        let prev_paid = superseded.as_ref().map(|d| d.total_amount);
        let bdue = prev_paid.map(|p| booking.total_price - p);
        (sid, prev_paid, bdue)
    } else {
        (None, None, None)
    };

    let doc_num = common::tax_document::document_number(
        common::tax_document::TaxDocumentType::PaymentReceipt,
        issue_date.year(),
        &booking.confirmation_code,
        revision,
    );

    // Fetch recipient user details
    let user = sqlx::query!(
        r#"SELECT first_name, last_name, email FROM "user" WHERE id = $1"#,
        booking.guest_id
    )
    .fetch_one(&mut *conn)
    .await?;

    let full_name = format!("{} {}", user.first_name, user.last_name)
        .trim()
        .to_string();
    let recipient_name = if full_name.is_empty() {
        user.email.clone()
    } else {
        full_name
    };

    let accommodation_net =
        booking.sub_total_price - booking.discount_value.unwrap_or(Decimal::ZERO);
    let cleaning_fee = booking.cleaning_fee;
    let taxable_amount = accommodation_net + cleaning_fee;
    let tax_rate = booking.tax_rate;
    let tax_amount = booking
        .tax_value
        .unwrap_or_else(|| (taxable_amount * tax_rate).round_dp(2));
    let platform_fee = booking.platform_fee;
    let damage_deposit = booking.damage_deposit;
    let total_amount = booking.total_price;

    let jmd_tax_amount = (tax_amount * jmd_exchange_rate).round_dp(2);
    let jmd_total_amount = (total_amount * jmd_exchange_rate).round_dp(2);

    let mut line_items = Vec::new();
    line_items.push(common::tax_document::InvoiceLineItem {
        description: format!("Accommodation ({} nights)", booking.total_days),
        quantity: booking.total_days.max(1) as u32,
        unit_price: if booking.total_days > 0 {
            (accommodation_net / Decimal::from(booking.total_days)).round_dp(2)
        } else {
            accommodation_net
        },
        tax_category: common::tax_document::InvoiceTaxCategory::Gct,
        tax_rate,
        tax_amount: (accommodation_net * tax_rate).round_dp(2),
        total_amount: accommodation_net + (accommodation_net * tax_rate).round_dp(2),
    });

    if cleaning_fee > Decimal::ZERO {
        line_items.push(common::tax_document::InvoiceLineItem {
            description: "Cleaning fee".to_string(),
            quantity: 1,
            unit_price: cleaning_fee,
            tax_category: common::tax_document::InvoiceTaxCategory::Gct,
            tax_rate,
            tax_amount: (cleaning_fee * tax_rate).round_dp(2),
            total_amount: cleaning_fee + (cleaning_fee * tax_rate).round_dp(2),
        });
    }

    if platform_fee > Decimal::ZERO {
        line_items.push(common::tax_document::InvoiceLineItem {
            description: "Platform fee".to_string(),
            quantity: 1,
            unit_price: platform_fee,
            tax_category: common::tax_document::InvoiceTaxCategory::Exempt,
            tax_rate: Decimal::ZERO,
            tax_amount: Decimal::ZERO,
            total_amount: platform_fee,
        });
    }

    if damage_deposit > Decimal::ZERO {
        line_items.push(common::tax_document::InvoiceLineItem {
            description: "Security deposit (held)".to_string(),
            quantity: 1,
            unit_price: damage_deposit,
            tax_category: common::tax_document::InvoiceTaxCategory::Exempt,
            tax_rate: Decimal::ZERO,
            tax_amount: Decimal::ZERO,
            total_amount: damage_deposit,
        });
    }

    let line_items_val = serde_json::to_value(&line_items)
        .map_err(|e| crate::error::DbError::ValidationError(e.to_string()))?;

    let new_doc = NewTaxDocument {
        document_type: TaxDocumentType::PaymentReceipt,
        document_number: doc_num,
        revision,
        status: TaxDocumentStatus::Issued,
        supersedes_id,
        booking_id: booking.id,
        booking_history_id,
        recipient_id: booking.guest_id,
        listing_id: booking.listing_id,
        issue_date,
        supplier_name: supplier.supplier_name.clone(),
        supplier_address: supplier.supplier_address.clone(),
        supplier_trn: supplier.supplier_trn.clone(),
        recipient_name,
        recipient_email: user.email,
        recipient_company_name: booking.metadata.business_company_name.clone(),
        recipient_trn: booking.metadata.business_tax_id.clone(),
        currency: booking.currency.clone(),
        exchange_rate: booking.exchange_rate,
        jmd_exchange_rate,
        accommodation_net,
        cleaning_fee,
        taxable_amount,
        tax_rate,
        tax_amount,
        platform_fee,
        damage_deposit,
        total_amount,
        amount_previously_paid,
        balance_due,
        refund_pct: None,
        retained_amount: None,
        jmd_tax_amount,
        jmd_total_amount,
        line_items: line_items_val,
    };

    insert_tax_document(conn, &new_doc).await
}

/// Retrieves the active (`status = 'issued'`) document of the specified type for a booking.
pub async fn get_active_by_type<'e, E>(
    executor: E,
    booking_id: Uuid,
    doc_type: TaxDocumentType,
) -> Result<Option<TaxDocumentRow>>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query_as!(
        TaxDocumentRow,
        r#"
        SELECT
            id,
            document_type as "document_type: TaxDocumentType",
            document_number,
            revision,
            status as "status: TaxDocumentStatus",
            supersedes_id,
            booking_id,
            booking_history_id,
            recipient_id,
            listing_id,
            issue_date,
            supplier_name,
            supplier_address,
            supplier_trn,
            recipient_name,
            recipient_email,
            recipient_company_name,
            recipient_trn,
            currency,
            exchange_rate,
            jmd_exchange_rate,
            accommodation_net,
            cleaning_fee,
            taxable_amount,
            tax_rate,
            tax_amount,
            platform_fee,
            damage_deposit,
            total_amount,
            amount_previously_paid,
            balance_due,
            refund_pct,
            retained_amount,
            jmd_tax_amount,
            jmd_total_amount,
            line_items,
            created_at
        FROM tax_document
        WHERE booking_id = $1 AND document_type = $2 AND status = 'issued'::tax_document_status
        "#,
        booking_id,
        doc_type as TaxDocumentType
    )
    .fetch_optional(executor)
    .await?;

    Ok(row)
}

/// Issues a statutory refund receipt (`REF-YYYY-{code}`) upon booking cancellation.
pub async fn issue_refund_receipt(
    conn: &mut PgConnection,
    booking: &Booking,
    supplier: &SupplierInfo,
    booking_history_id: Option<Uuid>,
) -> Result<TaxDocumentRow> {
    let issue_date = Utc::now();

    // Look up JMD FX rate
    let jmd_exchange_rate =
        crate::exchange_rate::get_rate(&mut *conn, &booking.currency, "JMD", issue_date).await?;

    let doc_num = common::tax_document::document_number(
        common::tax_document::TaxDocumentType::RefundReceipt,
        issue_date.year(),
        &booking.confirmation_code,
        1,
    );

    // Fetch recipient user details
    let user = sqlx::query!(
        r#"SELECT first_name, last_name, email FROM "user" WHERE id = $1"#,
        booking.guest_id
    )
    .fetch_one(&mut *conn)
    .await?;

    let full_name = format!("{} {}", user.first_name, user.last_name)
        .trim()
        .to_string();
    let recipient_name = if full_name.is_empty() {
        user.email.clone()
    } else {
        full_name
    };

    let recipient_company_name = booking.metadata.business_company_name.clone();
    let recipient_trn = booking.metadata.business_tax_id.clone();

    let accommodation_net =
        booking.sub_total_price - booking.discount_value.unwrap_or(Decimal::ZERO);
    let amounts = common::cancellation::RefundableAmounts::new(
        accommodation_net,
        booking.cleaning_fee,
        booking.damage_deposit,
        booking.platform_fee,
        booking.tax_rate,
        booking.total_price,
    );

    let quote = common::cancellation::quote_refund(
        booking.cancellation_policy.into(),
        amounts,
        booking.created_at,
        booking.date_from,
        issue_date,
    )
    .map_err(|e| crate::error::DbError::ValidationError(e.to_string()))?;

    let mut line_items = Vec::new();
    if quote.accommodation_refund > Decimal::ZERO {
        let acc_tax = (quote.accommodation_refund * booking.tax_rate).round_dp(2);
        line_items.push(common::tax_document::InvoiceLineItem {
            description: format!(
                "Accommodation refund ({:.0}% policy schedule)",
                quote.policy_pct * Decimal::from(100)
            ),
            quantity: 1,
            unit_price: quote.accommodation_refund,
            tax_category: common::tax_document::InvoiceTaxCategory::Gct,
            tax_rate: booking.tax_rate,
            tax_amount: acc_tax,
            total_amount: quote.accommodation_refund + acc_tax,
        });
    }

    if quote.cleaning_refund > Decimal::ZERO {
        let clean_tax = (quote.cleaning_refund * booking.tax_rate).round_dp(2);
        line_items.push(common::tax_document::InvoiceLineItem {
            description: "Cleaning fee refund".to_string(),
            quantity: 1,
            unit_price: quote.cleaning_refund,
            tax_category: common::tax_document::InvoiceTaxCategory::Gct,
            tax_rate: booking.tax_rate,
            tax_amount: clean_tax,
            total_amount: quote.cleaning_refund + clean_tax,
        });
    }

    if quote.deposit_refund > Decimal::ZERO {
        line_items.push(common::tax_document::InvoiceLineItem {
            description: "Security deposit refund (released)".to_string(),
            quantity: 1,
            unit_price: quote.deposit_refund,
            tax_category: common::tax_document::InvoiceTaxCategory::Exempt,
            tax_rate: Decimal::ZERO,
            tax_amount: Decimal::ZERO,
            total_amount: quote.deposit_refund,
        });
    }

    if quote.platform_fee_refund > Decimal::ZERO {
        line_items.push(common::tax_document::InvoiceLineItem {
            description: "Platform fee refund".to_string(),
            quantity: 1,
            unit_price: quote.platform_fee_refund,
            tax_category: common::tax_document::InvoiceTaxCategory::Exempt,
            tax_rate: Decimal::ZERO,
            tax_amount: Decimal::ZERO,
            total_amount: quote.platform_fee_refund,
        });
    }

    let line_items_val = serde_json::to_value(&line_items)
        .map_err(|e| crate::error::DbError::ValidationError(e.to_string()))?;

    let jmd_tax_amount = (quote.tax_refund * jmd_exchange_rate).round_dp(2);
    let jmd_total_amount = (quote.refund_total * jmd_exchange_rate).round_dp(2);

    let new_doc = NewTaxDocument {
        document_type: TaxDocumentType::RefundReceipt,
        document_number: doc_num,
        revision: 1,
        status: TaxDocumentStatus::Issued,
        supersedes_id: None,
        booking_id: booking.id,
        booking_history_id,
        recipient_id: booking.guest_id,
        listing_id: booking.listing_id,
        issue_date,
        supplier_name: supplier.supplier_name.clone(),
        supplier_address: supplier.supplier_address.clone(),
        supplier_trn: supplier.supplier_trn.clone(),
        recipient_name,
        recipient_email: user.email,
        recipient_company_name,
        recipient_trn,
        currency: booking.currency.clone(),
        exchange_rate: booking.exchange_rate,
        jmd_exchange_rate,
        accommodation_net: quote.accommodation_refund,
        cleaning_fee: quote.cleaning_refund,
        taxable_amount: quote.accommodation_refund + quote.cleaning_refund,
        tax_rate: booking.tax_rate,
        tax_amount: quote.tax_refund,
        platform_fee: quote.platform_fee_refund,
        damage_deposit: quote.deposit_refund,
        total_amount: quote.refund_total,
        amount_previously_paid: None,
        balance_due: None,
        refund_pct: Some(quote.policy_pct),
        retained_amount: Some(quote.retained_total),
        jmd_tax_amount,
        jmd_total_amount,
        line_items: line_items_val,
    };

    insert_tax_document(conn, &new_doc).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BookingStatus;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;
    use sqlx::types::Json;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::Path;

    async fn setup_test_db() -> TestPg {
        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        TestPg::new(db_url, Path::new("migrations"))
    }

    #[tokio::test]
    async fn test_issue_payment_receipt_and_supersede() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        // 1. Setup host user
        let host_id = Uuid::now_v7();
        let host_user = crate::models::NewUser {
            id: host_id,
            email: format!("host_{}@example.com", host_id),
            password_hash: "hash".to_string(),
            first_name: "Host".to_string(),
            last_name: "Hostson".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![crate::models::UserRole::Host]),
            default_currency: "USD".to_string(),
        };
        crate::user::create_user(&mut *tx, &host_user)
            .await
            .expect("Failed to create host user");

        let profile = crate::models::NewHostProfile {
            description: Some("Host".to_string()),
            verified_status: Some("pending".to_string()),
            payout_details: None,
        };
        crate::user::create_host_profile(&mut *tx, host_id, &profile)
            .await
            .expect("Failed to create host profile");

        // 2. Setup listing
        let listing = crate::listing::create_listing(
            &mut *tx,
            &crate::models::NewListing {
                name: format!("Tax Test Villa {}", Uuid::now_v7()),
                user_id: host_id,
                description: None,
                listing_structure_id: 1,
                country: "JM".to_string(),
                price_per_night: Some(dec!(200.00)),
                weekly_discount_percentage: None,
                monthly_discount_percentage: None,
                max_guests: 4,
                bedrooms: 2,
                beds: 2,
                full_bathrooms: 1,
                half_bathrooms: 0,
                square_meters: None,
                latitude: None,
                longitude: None,
                listing_details: None,
                city: None,
                base_currency: "USD".to_string(),
                minimum_stay: 1,
                days_between_bookings: 0,
                commission_pct: Some(dec!(0.1000)),
                cleaning_fee: Some(dec!(50.00)),
                damage_deposit: Some(dec!(100.00)),
            },
        )
        .await
        .expect("Failed to create listing");

        // 3. Setup guest user
        let guest_id = Uuid::now_v7();
        let guest_user = crate::models::NewUser {
            id: guest_id,
            email: format!("guest_{}@example.com", guest_id),
            password_hash: "hash".to_string(),
            first_name: "Jane".to_string(),
            last_name: "Doe".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![crate::models::UserRole::Booker]),
            default_currency: "USD".to_string(),
        };
        crate::user::create_user(&mut *tx, &guest_user)
            .await
            .expect("Failed to create guest user");

        // 4. Ensure USD -> JMD exchange rate exists
        sqlx::query!(
            r#"
            INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
            VALUES ('USD', 'JMD', 160.000000, NOW())
            ON CONFLICT (base_currency, target_currency, effective_at) DO NOTHING
            "#
        )
        .execute(&mut *tx)
        .await
        .expect("Failed to ensure exchange rate");

        // 5. Insert test booking
        let booking_id = Uuid::now_v7();
        let code = "TEST01";
        let booking = sqlx::query_as!(
            Booking,
            r#"
            INSERT INTO booking (
                id, confirmation_code, guest_id, listing_id, status,
                date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                sub_total_price, discount_value, tax_value, fee_breakdown,
                total_price, cancellation_policy, metadata, door_access_code,
                cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
                created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, 'confirmed'::booking_status,
                $5, $6, 'USD', 200.00, 2, 3,
                600.00, 0.00, 97.50, '[]'::jsonb,
                827.50, 'flexible'::cancellation_policy, $7, NULL,
                50.00, 100.00, 30.00, 0.1500, 1.000000,
                NOW(), NOW()
            )
            RETURNING
                id, confirmation_code, guest_id, listing_id, status as "status: BookingStatus",
                date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<crate::models::FeeItem>>",
                total_price, cancellation_policy as "cancellation_policy: crate::models::CancellationPolicy",
                metadata as "metadata: Json<crate::models::BookingMetadata>",
                door_access_code,
                cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
                created_at, updated_at
            "#,
            booking_id,
            code,
            guest_id,
            listing.id,
            NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 4).unwrap(),
            Json(crate::models::BookingMetadata {
                num_adults: 2,
                num_children: 0,
                num_infants: 0,
                num_pets: 0,
                message_to_host: None,
                estimated_arrival_time: None,
                is_business_trip: true,
                business_company_name: Some("Acme Corp".to_string()),
                business_tax_id: Some("123-456-789".to_string()),
            }) as _
        )
        .fetch_one(&mut *tx)
        .await
        .expect("Failed to insert test booking");

        let supplier = SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        };

        // 6. Issue first payment receipt (Revision 1)
        let doc1 = issue_payment_receipt(&mut tx, &booking, &supplier, None)
            .await
            .expect("Failed to issue payment receipt rev 1");

        assert_eq!(doc1.document_type, TaxDocumentType::PaymentReceipt);
        assert_eq!(doc1.revision, 1);
        assert_eq!(doc1.status, TaxDocumentStatus::Issued);
        assert_eq!(doc1.supersedes_id, None);
        assert_eq!(doc1.total_amount, dec!(827.50));
        assert_eq!(doc1.jmd_exchange_rate, dec!(160.000000));
        assert_eq!(doc1.jmd_total_amount, dec!(132400.00)); // 827.50 * 160
        assert_eq!(doc1.recipient_company_name.as_deref(), Some("Acme Corp"));
        assert_eq!(doc1.recipient_trn.as_deref(), Some("123-456-789"));
        assert_eq!(doc1.document_number, format!("REC-2026-{}", code));

        // 7. Verify list_for_booking
        let docs = list_for_booking(&mut *tx, booking.id)
            .await
            .expect("Failed to list docs");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].id, doc1.id);

        // 8. Verify get_by_number
        let by_num = get_by_number(&mut *tx, booking.id, &doc1.document_number)
            .await
            .expect("Failed to query doc by number")
            .expect("Doc not found by number");
        assert_eq!(by_num.id, doc1.id);

        // 9. Issue second payment receipt (Revision 2 - e.g. after modification)
        let doc2 = issue_payment_receipt(&mut tx, &booking, &supplier, None)
            .await
            .expect("Failed to issue payment receipt rev 2");

        assert_eq!(doc2.revision, 2);
        assert_eq!(doc2.status, TaxDocumentStatus::Issued);
        assert_eq!(doc2.supersedes_id, Some(doc1.id));
        assert_eq!(doc2.amount_previously_paid, Some(dec!(827.50)));
        assert_eq!(doc2.balance_due, Some(dec!(0.00)));
        assert_eq!(doc2.document_number, format!("REC-2026-{}-R2", code));

        // 10. Verify previous document status is now Superseded
        let old_doc = get_by_number(&mut *tx, booking.id, &doc1.document_number)
            .await
            .expect("Failed to query old doc")
            .expect("Old doc not found");
        assert_eq!(old_doc.status, TaxDocumentStatus::Superseded);

        // 11. Issue refund receipt on cancellation (REF-2026-{code})
        let ref_doc = issue_refund_receipt(&mut tx, &booking, &supplier, None)
            .await
            .expect("Failed to issue refund receipt");

        assert_eq!(ref_doc.document_type, TaxDocumentType::RefundReceipt);
        assert_eq!(ref_doc.document_number, format!("REF-2026-{}", code));
        assert_eq!(ref_doc.revision, 1);
        assert_eq!(ref_doc.status, TaxDocumentStatus::Issued);
        assert!(ref_doc.refund_pct.is_some());
        assert!(ref_doc.retained_amount.is_some());
        assert_eq!(
            ref_doc.total_amount + ref_doc.retained_amount.unwrap(),
            booking.total_price
        );
    }
}
