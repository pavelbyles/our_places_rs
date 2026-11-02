//! Booking lifecycle integration tests.

use super::unavailable_ranges_tests::HOLD_MINUTES;
use super::{create_booking, get_booking_by_id, get_bookings_by_listing_id, update_booking};
use crate::models::{BookingStatus, CancellationPolicy, NewBooking, SupplierInfo, UpdatedBooking};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx_db_tester::TestPg;
use std::env;
use std::path::Path;
use uuid::Uuid;

async fn setup_test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string());
    TestPg::new(db_url, Path::new("migrations"))
}

#[tokio::test]
async fn test_guest_pending_hold_cancellation_on_new_booking() {
    let test_db = setup_test_db().await;
    let pool = test_db.get_pool().await;
    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    // Create host user with host profile
    let host_id = Uuid::now_v7();
    let host_user = crate::models::NewUser {
        id: host_id,
        email: format!("host_{}@example.com", host_id),
        password_hash: "hash".to_string(),
        first_name: "Host".to_string(),
        last_name: "User".to_string(),
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
        .unwrap();

    let profile = crate::models::NewHostProfile {
        description: Some("Host".to_string()),
        verified_status: Some("pending".to_string()),
        payout_details: None,
    };
    crate::user::create_host_profile(&mut *tx, host_id, &profile)
        .await
        .unwrap();

    let nl1 = crate::models::NewListing {
        name: format!("Villa 1 {}", Uuid::now_v7()),
        user_id: host_id,
        description: None,
        listing_structure_id: 1,
        country: "Jamaica".to_string(),
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
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
    };
    let listing1 = crate::listing::create_listing(&mut *tx, &nl1)
        .await
        .unwrap();

    let nl2 = crate::models::NewListing {
        name: format!("Villa 2 {}", Uuid::now_v7()),
        user_id: host_id,
        description: None,
        listing_structure_id: 1,
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(300.00)),
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
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
    };
    let listing2 = crate::listing::create_listing(&mut *tx, &nl2)
        .await
        .unwrap();

    let guest_id = Uuid::now_v7();
    let guest_user = crate::models::NewUser {
        id: guest_id,
        email: format!("guest_{}@example.com", guest_id),
        password_hash: "hash".to_string(),
        first_name: "Guest".to_string(),
        last_name: "User".to_string(),
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
        .unwrap();

    tx.commit().await.unwrap();

    // Guest places Booking 1 on Villa 1
    let code1 = format!("C{}", Uuid::now_v7().simple())[..10].to_uppercase();
    let new_booking1 = NewBooking {
        confirmation_code: code1,
        guest_id,
        listing_id: listing1.id,
        date_from: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
        currency: "USD".to_string(),
        daily_rate: dec!(200.00),
        number_of_persons: 2,
        total_days: 4,
        sub_total_price: dec!(800.00),
        discount_value: None,
        tax_value: None,
        fee_breakdown: vec![],
        total_price: dec!(800.00),
        cancellation_policy: CancellationPolicy::Flexible,
        metadata: crate::models::BookingMetadata {
            num_adults: 2,
            ..Default::default()
        },
        door_access_code: None,
        cleaning_fee: Decimal::ZERO,
        damage_deposit: Decimal::ZERO,
        platform_fee: Decimal::ZERO,
        tax_rate: dec!(0.1000),
        exchange_rate: Decimal::ONE,
        promo: None,
    };

    let b1 = create_booking(&pool, &new_booking1, HOLD_MINUTES)
        .await
        .expect("Failed to create booking 1");
    assert_eq!(b1.status, BookingStatus::Pending);

    // Guest places Booking 2 on Villa 2
    let code2 = format!("D{}", Uuid::now_v7().simple())[..10].to_uppercase();
    let new_booking2 = NewBooking {
        confirmation_code: code2,
        guest_id,
        listing_id: listing2.id,
        date_from: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 11, 15).unwrap(),
        currency: "USD".to_string(),
        daily_rate: dec!(300.00),
        number_of_persons: 2,
        total_days: 5,
        sub_total_price: dec!(1500.00),
        discount_value: None,
        tax_value: None,
        fee_breakdown: vec![],
        total_price: dec!(1500.00),
        cancellation_policy: CancellationPolicy::Flexible,
        metadata: crate::models::BookingMetadata {
            num_adults: 2,
            ..Default::default()
        },
        door_access_code: None,
        cleaning_fee: Decimal::ZERO,
        damage_deposit: Decimal::ZERO,
        platform_fee: Decimal::ZERO,
        tax_rate: dec!(0.1000),
        exchange_rate: Decimal::ONE,
        promo: None,
    };

    let b2 = create_booking(&pool, &new_booking2, HOLD_MINUTES)
        .await
        .expect("Failed to create booking 2");
    assert_eq!(b2.status, BookingStatus::Pending);

    // Verify Booking 1 on Villa 1 was automatically CANCELLED
    let b1_updated = get_booking_by_id(&pool, b1.id).await.unwrap();
    assert_eq!(b1_updated.status, BookingStatus::Cancelled);

    // Test get_bookings_by_listing_id for listing1
    let listing1_bookings = get_bookings_by_listing_id(&pool, listing1.id, 1, 10)
        .await
        .expect("Failed to fetch bookings for listing1");
    assert_eq!(listing1_bookings.len(), 1);
    assert_eq!(listing1_bookings[0].id, b1.id);

    // Test get_bookings_by_listing_id for listing2
    let listing2_bookings = get_bookings_by_listing_id(&pool, listing2.id, 1, 10)
        .await
        .expect("Failed to fetch bookings for listing2");
    assert_eq!(listing2_bookings.len(), 1);
    assert_eq!(listing2_bookings[0].id, b2.id);
}

#[tokio::test]
async fn test_update_booking_confirmed_issues_payment_receipt() {
    let test_db = setup_test_db().await;
    let pool = test_db.get_pool().await;

    let host_id = Uuid::now_v7();
    let guest_id = Uuid::now_v7();

    let mut tx = pool.begin().await.unwrap();

    let host_user = crate::models::NewUser {
        id: host_id,
        email: format!("host_{}@example.com", host_id),
        password_hash: "hash".to_string(),
        first_name: "TaxHost".to_string(),
        last_name: "Owner".to_string(),
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
        .unwrap();

    let profile = crate::models::NewHostProfile {
        description: Some("Host".to_string()),
        verified_status: Some("pending".to_string()),
        payout_details: None,
    };
    crate::user::create_host_profile(&mut *tx, host_id, &profile)
        .await
        .unwrap();

    let guest_user = crate::models::NewUser {
        id: guest_id,
        email: format!("guest_{}@example.com", guest_id),
        password_hash: "hash".to_string(),
        first_name: "TaxGuest".to_string(),
        last_name: "Booker".to_string(),
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
        .unwrap();

    let listing = crate::listing::create_listing(
        &mut *tx,
        &crate::models::NewListing {
            name: format!("Receipt Test Villa {}", Uuid::now_v7()),
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
            guests_included: None,
            extra_guest_fee: None,
        },
    )
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
        VALUES ('USD', 'JMD', 160.000000, NOW())
        ON CONFLICT (base_currency, target_currency, effective_at) DO NOTHING
        "#
    )
    .execute(&mut *tx)
    .await
    .unwrap();

    tx.commit().await.unwrap();

    let code = format!("R{}", Uuid::now_v7().simple())[..10].to_uppercase();
    let new_booking = NewBooking {
        confirmation_code: code.clone(),
        guest_id,
        listing_id: listing.id,
        date_from: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 12, 5).unwrap(),
        currency: "USD".to_string(),
        daily_rate: dec!(200.00),
        number_of_persons: 2,
        total_days: 4,
        sub_total_price: dec!(800.00),
        discount_value: None,
        tax_value: Some(dec!(127.50)),
        fee_breakdown: vec![],
        total_price: dec!(1077.50),
        cancellation_policy: CancellationPolicy::Flexible,
        metadata: crate::models::BookingMetadata {
            num_adults: 2,
            business_company_name: Some("Global Corp".to_string()),
            business_tax_id: Some("999-888-777".to_string()),
            ..Default::default()
        },
        door_access_code: None,
        cleaning_fee: dec!(50.00),
        damage_deposit: dec!(100.00),
        platform_fee: dec!(40.00),
        tax_rate: dec!(0.1500),
        exchange_rate: Decimal::ONE,
        promo: None,
    };

    let booking = create_booking(&pool, &new_booking, HOLD_MINUTES)
        .await
        .unwrap();
    assert_eq!(booking.status, BookingStatus::Pending);

    let supplier = SupplierInfo {
        supplier_name: "Our Places Jamaica Limited".to_string(),
        supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
        supplier_trn: "000-111-222".to_string(),
    };

    // Transition from Pending to Confirmed
    let updated = UpdatedBooking {
        status: Some(BookingStatus::Confirmed),
        ..Default::default()
    };

    let confirmed_booking = update_booking(&pool, booking.id, &updated, &supplier)
        .await
        .expect("Failed to confirm booking");
    assert_eq!(confirmed_booking.status, BookingStatus::Confirmed);

    // Verify that exactly one payment receipt document was created
    let docs = crate::tax_document::list_for_booking(&pool, booking.id)
        .await
        .expect("Failed to list tax documents");
    assert_eq!(docs.len(), 1);
    let doc = &docs[0];
    assert_eq!(
        doc.document_type,
        crate::models::TaxDocumentType::PaymentReceipt
    );
    assert_eq!(doc.total_amount, dec!(1077.50));
    assert_eq!(doc.jmd_total_amount, dec!(172400.00)); // 1077.50 * 160
    assert_eq!(doc.recipient_company_name.as_deref(), Some("Global Corp"));
    assert_eq!(doc.recipient_trn.as_deref(), Some("999-888-777"));
    assert_eq!(doc.document_number, format!("REC-2026-{}", code));

    // Transition from Confirmed to Cancelled
    let cancel_update = UpdatedBooking {
        status: Some(BookingStatus::Cancelled),
        ..Default::default()
    };
    let cancelled_booking = update_booking(&pool, booking.id, &cancel_update, &supplier)
        .await
        .expect("Failed to cancel confirmed booking");
    assert_eq!(cancelled_booking.status, BookingStatus::Cancelled);

    // Verify that a refund receipt document was created
    let docs_after_cancel = crate::tax_document::list_for_booking(&pool, booking.id)
        .await
        .expect("Failed to list tax documents after cancel");
    assert_eq!(docs_after_cancel.len(), 2);
    let ref_doc = docs_after_cancel
        .iter()
        .find(|d| d.document_type == crate::models::TaxDocumentType::RefundReceipt)
        .expect("Refund receipt document not found");
    assert_eq!(ref_doc.document_number, format!("REF-2026-{}", code));
    assert!(ref_doc.refund_pct.is_some());
    assert!(ref_doc.retained_amount.is_some());
    assert_eq!(
        ref_doc.total_amount + ref_doc.retained_amount.unwrap(),
        booking.total_price
    );

    // Cleanup created data to preserve pagination baseline for other tests
    let _ = sqlx::query!("DELETE FROM tax_document WHERE booking_id = $1", booking.id)
        .execute(&pool)
        .await;
    let _ = sqlx::query!(
        "DELETE FROM host_payout_ledger WHERE booking_id = $1",
        booking.id
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query!(
        "DELETE FROM booking_history WHERE booking_id = $1",
        booking.id
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query!("DELETE FROM booking WHERE id = $1", booking.id)
        .execute(&pool)
        .await;
    let _ = sqlx::query!("DELETE FROM listing WHERE id = $1", listing.id)
        .execute(&pool)
        .await;
    let _ = sqlx::query!("DELETE FROM host_profiles WHERE user_id = $1", host_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query!(
        "DELETE FROM \"user\" WHERE id = $1 OR id = $2",
        host_id,
        guest_id
    )
    .execute(&pool)
    .await;
}

#[tokio::test]
async fn test_update_booking_date_change_and_revised_receipt() {
    let test_db = setup_test_db().await;
    let pool = test_db.get_pool().await;
    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    let host_id = Uuid::now_v7();
    let host_user = crate::models::NewUser {
        id: host_id,
        email: format!("host_{}@example.com", host_id),
        password_hash: "hash".to_string(),
        first_name: "Host".to_string(),
        last_name: "User".to_string(),
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
        .unwrap();

    let profile = crate::models::NewHostProfile {
        description: Some("Host".to_string()),
        verified_status: Some("pending".to_string()),
        payout_details: None,
    };
    crate::user::create_host_profile(&mut *tx, host_id, &profile)
        .await
        .unwrap();

    let nl = crate::models::NewListing {
        name: format!("Date Change Villa {}", Uuid::now_v7()),
        user_id: host_id,
        description: None,
        listing_structure_id: 1,
        country: "Jamaica".to_string(),
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
        guests_included: None,
        extra_guest_fee: None,
    };
    let listing = crate::listing::create_listing(&mut *tx, &nl).await.unwrap();

    let guest_id = Uuid::now_v7();
    let guest_user = crate::models::NewUser {
        id: guest_id,
        email: format!("guest_{}@example.com", guest_id),
        password_hash: "hash".to_string(),
        first_name: "Guest".to_string(),
        last_name: "User".to_string(),
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
        .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
        VALUES ('USD', 'JMD', 160.000000, NOW())
        ON CONFLICT (base_currency, target_currency, effective_at) DO NOTHING
        "#
    )
    .execute(&mut *tx)
    .await
    .unwrap();

    tx.commit().await.unwrap();

    let code = format!("R{}", Uuid::new_v4().simple())[..10].to_uppercase();
    let new_booking = NewBooking {
        confirmation_code: code.clone(),
        guest_id,
        listing_id: listing.id,
        date_from: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
        currency: "USD".to_string(),
        daily_rate: dec!(200.00),
        number_of_persons: 2,
        total_days: 4,
        sub_total_price: dec!(800.00),
        discount_value: None,
        tax_value: Some(dec!(127.50)),
        fee_breakdown: vec![],
        total_price: dec!(1077.50),
        cancellation_policy: CancellationPolicy::Flexible,
        metadata: crate::models::BookingMetadata {
            num_adults: 2,
            ..Default::default()
        },
        door_access_code: None,
        cleaning_fee: dec!(50.00),
        damage_deposit: dec!(100.00),
        platform_fee: dec!(40.00),
        tax_rate: dec!(0.1500),
        exchange_rate: Decimal::ONE,
        promo: None,
    };

    let booking = create_booking(&pool, &new_booking, HOLD_MINUTES)
        .await
        .unwrap();

    let supplier = SupplierInfo {
        supplier_name: "Our Places Jamaica Limited".to_string(),
        supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
        supplier_trn: "000-111-222".to_string(),
    };

    // Confirm booking -> Issues R1 (PaymentReceipt)
    let confirm_update = UpdatedBooking {
        status: Some(BookingStatus::Confirmed),
        ..Default::default()
    };
    let confirmed = update_booking(&pool, booking.id, &confirm_update, &supplier)
        .await
        .expect("Failed to confirm booking");
    assert_eq!(confirmed.status, BookingStatus::Confirmed);

    let initial_docs = crate::tax_document::list_for_booking(&pool, booking.id)
        .await
        .unwrap();
    assert_eq!(initial_docs.len(), 1);
    assert_eq!(initial_docs[0].revision, 1);
    assert_eq!(
        initial_docs[0].document_number,
        format!("REC-2026-{}", code)
    );
    assert_eq!(
        initial_docs[0].status,
        crate::models::TaxDocumentStatus::Issued
    );

    // Date Change: extend stay by 2 days (Nov 1 - Nov 7, 6 nights)
    let date_change = UpdatedBooking {
        date_from: Some(NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()),
        date_to: Some(NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()),
        daily_rate: Some(dec!(200.00)),
        total_days: Some(6),
        sub_total_price: Some(dec!(1200.00)),
        discount_value: None,
        tax_value: Some(dec!(187.50)),
        fee_breakdown: Some(vec![]),
        total_price: Some(dec!(1537.50)),
        cleaning_fee: Some(dec!(50.00)),
        damage_deposit: Some(dec!(100.00)),
        platform_fee: Some(dec!(60.00)),
        tax_rate: Some(dec!(0.1500)),
        exchange_rate: Some(Decimal::ONE),
        ..Default::default()
    };

    let updated_booking = update_booking(&pool, booking.id, &date_change, &supplier)
        .await
        .expect("Failed to update booking dates");
    assert_eq!(updated_booking.total_days, 6);
    assert_eq!(updated_booking.total_price, dec!(1537.50));
    assert_eq!(
        updated_booking.date_to,
        NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()
    );

    // Verify revised receipt: R1 is superseded, R2 is active with amount_previously_paid and balance_due
    let docs = crate::tax_document::list_for_booking(&pool, booking.id)
        .await
        .unwrap();
    assert_eq!(docs.len(), 2);

    let r1 = docs.iter().find(|d| d.revision == 1).unwrap();
    assert_eq!(r1.status, crate::models::TaxDocumentStatus::Superseded);

    let r2 = docs.iter().find(|d| d.revision == 2).unwrap();
    assert_eq!(r2.status, crate::models::TaxDocumentStatus::Issued);
    assert_eq!(r2.document_number, format!("REC-2026-{}-R2", code));
    assert_eq!(r2.amount_previously_paid, Some(dec!(1077.50)));
    assert_eq!(r2.balance_due, Some(dec!(460.00))); // 1537.50 - 1077.50
    assert_eq!(r2.supersedes_id, Some(r1.id));

    // Create a 2nd booking: Nov 10 - Nov 15
    let code2 = format!("S{}", Uuid::new_v4().simple())[..10].to_uppercase();
    let second_booking = NewBooking {
        confirmation_code: code2,
        guest_id,
        listing_id: listing.id,
        date_from: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 11, 15).unwrap(),
        currency: "USD".to_string(),
        daily_rate: dec!(200.00),
        number_of_persons: 2,
        total_days: 5,
        sub_total_price: dec!(1000.00),
        discount_value: None,
        tax_value: Some(dec!(150.00)),
        fee_breakdown: vec![],
        total_price: dec!(1300.00),
        cancellation_policy: CancellationPolicy::Flexible,
        metadata: crate::models::BookingMetadata {
            num_adults: 2,
            ..Default::default()
        },
        door_access_code: None,
        cleaning_fee: dec!(50.00),
        damage_deposit: dec!(100.00),
        platform_fee: dec!(50.00),
        tax_rate: dec!(0.1500),
        exchange_rate: Decimal::ONE,
        promo: None,
    };
    let b2 = create_booking(&pool, &second_booking, HOLD_MINUTES)
        .await
        .unwrap();
    let _ = update_booking(&pool, b2.id, &confirm_update, &supplier)
        .await
        .unwrap();

    // Attempt to change booking 1 dates to overlap with booking 2 (Nov 9 - Nov 12)
    let overlapping_change = UpdatedBooking {
        date_from: Some(NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()),
        date_to: Some(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()),
        total_days: Some(3),
        total_price: Some(dec!(900.00)),
        ..Default::default()
    };
    let err = update_booking(&pool, booking.id, &overlapping_change, &supplier)
        .await
        .unwrap_err();
    match err {
        crate::error::DbError::DateUnavailable => {}
        other => panic!("Expected DateUnavailable, got: {:?}", other),
    }

    // Attempt date change on a Pending booking -> BookingNotModifiable
    let pending_req = NewBooking {
        confirmation_code: format!("P{}", Uuid::new_v4().simple())[..10].to_uppercase(),
        guest_id,
        listing_id: listing.id,
        date_from: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 12, 5).unwrap(),
        currency: "USD".to_string(),
        daily_rate: dec!(200.00),
        number_of_persons: 2,
        total_days: 4,
        sub_total_price: dec!(800.00),
        discount_value: None,
        tax_value: Some(dec!(127.50)),
        fee_breakdown: vec![],
        total_price: dec!(1077.50),
        cancellation_policy: CancellationPolicy::Flexible,
        metadata: crate::models::BookingMetadata {
            num_adults: 2,
            ..Default::default()
        },
        door_access_code: None,
        cleaning_fee: dec!(50.00),
        damage_deposit: dec!(100.00),
        platform_fee: dec!(40.00),
        tax_rate: dec!(0.1500),
        exchange_rate: Decimal::ONE,
        promo: None,
    };
    let pending_b = create_booking(&pool, &pending_req, HOLD_MINUTES)
        .await
        .unwrap();
    let pending_change = UpdatedBooking {
        date_from: Some(NaiveDate::from_ymd_opt(2026, 12, 2).unwrap()),
        date_to: Some(NaiveDate::from_ymd_opt(2026, 12, 6).unwrap()),
        ..Default::default()
    };
    let pending_err = update_booking(&pool, pending_b.id, &pending_change, &supplier)
        .await
        .unwrap_err();
    match pending_err {
        crate::error::DbError::BookingNotModifiable(_) => {}
        other => panic!("Expected BookingNotModifiable, got: {:?}", other),
    }

    // Clean up
    let _ = sqlx::query!(
        "DELETE FROM tax_document WHERE booking_id = $1 OR booking_id = $2 OR booking_id = $3",
        booking.id,
        b2.id,
        pending_b.id
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query!("DELETE FROM host_payout_ledger WHERE booking_id = $1 OR booking_id = $2 OR booking_id = $3", booking.id, b2.id, pending_b.id).execute(&pool).await;
    let _ = sqlx::query!(
        "DELETE FROM booking_history WHERE booking_id = $1 OR booking_id = $2 OR booking_id = $3",
        booking.id,
        b2.id,
        pending_b.id
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query!(
        "DELETE FROM booking WHERE id = $1 OR id = $2 OR id = $3",
        booking.id,
        b2.id,
        pending_b.id
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query!("DELETE FROM listing WHERE id = $1", listing.id)
        .execute(&pool)
        .await;
    let _ = sqlx::query!("DELETE FROM host_profiles WHERE user_id = $1", host_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query!(
        "DELETE FROM \"user\" WHERE id = $1 OR id = $2",
        host_id,
        guest_id
    )
    .execute(&pool)
    .await;
}
