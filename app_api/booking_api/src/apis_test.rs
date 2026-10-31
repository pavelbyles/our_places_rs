#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::apis::BookingCalculator;
    use crate::settings::{CronSettings, TaxSettings};
    use rust_decimal::Decimal;

    #[test]
    fn test_booking_calculator_state_machine() {
        let daily_rate = Decimal::new(100, 0);
        let total_days = 10;

        // BaseRate
        let calc = BookingCalculator::new(daily_rate, total_days);
        assert_eq!(calc.sub_total_price, Decimal::new(1000, 0));

        // BaseRate -> Discounted (Weekly discount 10%)
        let weekly_pct = Some(Decimal::new(10, 0));
        let discounted = calc.apply_discounts(None, weekly_pct);
        assert_eq!(discounted.discount_value.unwrap(), Decimal::new(100, 0));
        assert_eq!(discounted.discounted_subtotal, Decimal::new(900, 0));

        // Discounted -> Taxed (15% tax)
        let taxed = discounted.apply_taxes(Decimal::new(15, 2));
        assert_eq!(taxed.tax_value.unwrap(), Decimal::new(135, 0));

        // Taxed -> Final
        let final_calc = taxed.finalize();
        // Platform fee is 5% of discounted subtotal (900 * 0.05 = 45)
        assert_eq!(final_calc.fee_breakdown.len(), 1);
        assert_eq!(final_calc.fee_breakdown[0].amount, Decimal::new(45, 0));

        // Total price = 900 (subtotal) + 135 (tax) + 45 (fee) = 1080
        assert_eq!(final_calc.total_price, Decimal::new(1080, 0));
    }

    #[test]
    fn test_booking_calculator_with_seasonal_override() {
        use chrono::NaiveDate;
        use common::models::PriceOverride;
        use common::pricing::calculate_dynamic_quote;
        use uuid::Uuid;

        let base_rate = Decimal::new(200, 0); // $200
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 25).unwrap(); // 5 nights

        let overrides = vec![PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: NaiveDate::from_ymd_opt(2026, 12, 22).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 30).unwrap(),
            nightly_rate: Decimal::new(350, 0), // Peak rate $350
            min_nights: 2,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }];

        let quote = calculate_dynamic_quote(base_rate, 1, &overrides, check_in, check_out).unwrap();

        // 2 nights @ $200 (Dec 20, 21) + 3 nights @ $350 (Dec 22, 23, 24) = 400 + 1050 = 1450
        assert_eq!(quote.subtotal, Decimal::new(1450, 0));

        let calc = BookingCalculator::with_subtotal(quote.effective_daily_rate, 5, quote.subtotal)
            .apply_discounts(None, None)
            .apply_taxes(Decimal::new(15, 2))
            .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(1450, 0));
        assert_eq!(calc.tax_value, Some(Decimal::new(21750, 2))); // $217.50 (15% of 1450)
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(7250, 2)); // $72.50
        assert_eq!(calc.total_price, Decimal::new(174000, 2)); // 1450 + 217.50 + 72.50 = 1740.00
    }

    #[test]
    fn test_seasonal_min_nights_enforcement() {
        use chrono::NaiveDate;
        use common::models::PriceOverride;
        use common::pricing::{PricingError, calculate_dynamic_quote};
        use uuid::Uuid;

        let base_rate = Decimal::new(150, 0);
        let check_in = NaiveDate::from_ymd_opt(2026, 12, 24).unwrap();
        let check_out = NaiveDate::from_ymd_opt(2026, 12, 26).unwrap(); // 2 nights

        let overrides = vec![PriceOverride {
            id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            start_date: NaiveDate::from_ymd_opt(2026, 12, 20).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
            nightly_rate: Decimal::new(400, 0),
            min_nights: 3, // Requires 3 nights minimum for Christmas week
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }];

        let result = calculate_dynamic_quote(base_rate, 1, &overrides, check_in, check_out);
        assert_eq!(
            result,
            Err(PricingError::MinNightsNotMet {
                required: 3,
                provided: 2
            })
        );
    }

    #[test]
    fn test_updated_booking_request_json_deserialization() {
        use crate::apis::UpdatedBookingRequest;
        use db_core::models::BookingStatus;

        let json_data = r#"{"status":"confirmed"}"#;
        let parsed: Result<UpdatedBookingRequest, _> = serde_json::from_str(json_data);
        assert!(
            parsed.is_ok(),
            "Failed to deserialize status 'confirmed': {:?}",
            parsed.err()
        );
        assert_eq!(parsed.unwrap().status, Some(BookingStatus::Confirmed));

        let json_cancelled = r#"{"status":"cancelled"}"#;
        let parsed_cancelled: UpdatedBookingRequest = serde_json::from_str(json_cancelled).unwrap();
        assert_eq!(parsed_cancelled.status, Some(BookingStatus::Cancelled));

        let json_pending = r#"{"status":"pending"}"#;
        let parsed_pending: UpdatedBookingRequest = serde_json::from_str(json_pending).unwrap();
        assert_eq!(parsed_pending.status, Some(BookingStatus::Pending));

        let json_completed = r#"{"status":"completed"}"#;
        let parsed_completed: UpdatedBookingRequest = serde_json::from_str(json_completed).unwrap();
        assert_eq!(parsed_completed.status, Some(BookingStatus::Completed));
    }

    #[test]
    fn test_cancellation_policy_json_deserialization() {
        use db_core::models::CancellationPolicy;

        let flex: CancellationPolicy = serde_json::from_str(r#""flexible""#).unwrap();
        assert_eq!(flex, CancellationPolicy::Flexible);

        let mod_policy: CancellationPolicy = serde_json::from_str(r#""moderate""#).unwrap();
        assert_eq!(mod_policy, CancellationPolicy::Moderate);

        let strict: CancellationPolicy = serde_json::from_str(r#""strict""#).unwrap();
        assert_eq!(strict, CancellationPolicy::Strict);
    }

    #[test]
    fn test_user_bookings_response_mapping() {
        use crate::apis::map_booking_to_response;
        use chrono::Utc;
        use db_core::models::{Booking, BookingMetadata, BookingStatus, CancellationPolicy};
        use rust_decimal::Decimal;
        use sqlx::types::Json;
        use uuid::Uuid;

        let booking = Booking {
            id: Uuid::new_v4(),
            confirmation_code: "CONF-TEST123".to_string(),
            guest_id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            status: BookingStatus::Confirmed,
            date_from: chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            date_to: chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            currency: "USD".to_string(),
            daily_rate: Decimal::new(250, 0),
            number_of_persons: 2,
            total_days: 4,
            sub_total_price: Decimal::new(1000, 0),
            discount_value: None,
            tax_value: Some(Decimal::new(100, 0)),
            fee_breakdown: Json(vec![]),
            total_price: Decimal::new(1100, 0),
            cancellation_policy: CancellationPolicy::Flexible,
            metadata: Json(BookingMetadata {
                num_adults: 2,
                num_children: 0,
                num_infants: 0,
                num_pets: 0,
                message_to_host: Some("Looking forward to the stay!".to_string()),
                estimated_arrival_time: Some("15:00".to_string()),
                is_business_trip: false,
                business_company_name: None,
                business_tax_id: None,
            }),
            door_access_code: Some("1234".to_string()),
            cleaning_fee: Decimal::ZERO,
            damage_deposit: Decimal::ZERO,
            platform_fee: Decimal::new(45, 0),
            tax_rate: Decimal::new(15, 2),
            exchange_rate: Decimal::ONE,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let res = map_booking_to_response(booking);
        assert_eq!(res.confirmation_code, "CONF-TEST123");
        assert_eq!(res.status, "Confirmed");
        assert_eq!(res.door_access_code.as_deref(), Some("1234"));
        assert_eq!(res.tax_rate, Some(Decimal::new(15, 2)));
        assert_eq!(res.exchange_rate, Some(Decimal::ONE));
        assert_eq!(res.platform_fee, Some(Decimal::new(45, 0)));
        assert_eq!(res.total_price, Decimal::new(1100, 0));
        assert_eq!(res.metadata.num_adults, 2);
        assert_eq!(
            res.metadata.message_to_host.as_deref(),
            Some("Looking forward to the stay!")
        );
    }

    #[test]
    fn test_listing_bookings_response_mapping_all_statuses() {
        use crate::apis::map_booking_to_response;
        use chrono::Utc;
        use db_core::models::{Booking, BookingMetadata, BookingStatus, CancellationPolicy};
        use rust_decimal::Decimal;
        use sqlx::types::Json;
        use uuid::Uuid;

        let statuses = vec![
            (BookingStatus::Pending, "Pending"),
            (BookingStatus::Confirmed, "Confirmed"),
            (BookingStatus::Completed, "Completed"),
            (BookingStatus::Cancelled, "Cancelled"),
        ];

        for (status, expected_str) in statuses {
            let booking = Booking {
                id: Uuid::new_v4(),
                confirmation_code: "CONF-STATUS".to_string(),
                guest_id: Uuid::new_v4(),
                listing_id: Uuid::new_v4(),
                status,
                date_from: chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
                date_to: chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
                currency: "USD".to_string(),
                daily_rate: Decimal::new(200, 0),
                number_of_persons: 2,
                total_days: 4,
                sub_total_price: Decimal::new(800, 0),
                discount_value: None,
                tax_value: None,
                fee_breakdown: Json(vec![]),
                total_price: Decimal::new(800, 0),
                cancellation_policy: CancellationPolicy::Flexible,
                metadata: Json(BookingMetadata::default()),
                door_access_code: None,
                cleaning_fee: Decimal::ZERO,
                damage_deposit: Decimal::ZERO,
                platform_fee: Decimal::ZERO,
                tax_rate: Decimal::new(15, 2),
                exchange_rate: Decimal::ONE,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            let mapped = map_booking_to_response(booking);
            assert_eq!(mapped.status, expected_str);
        }
    }

    #[test]
    fn test_pagination_params_deserialization() {
        use api_core::pagination::Pagination;

        let json = r#"{"page":2,"per_page":25}"#;
        let parsed: Pagination = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.page, Some(2));
        assert_eq!(parsed.per_page, Some(25));

        let json_empty = "{}";
        let parsed_empty: Pagination = serde_json::from_str(json_empty).unwrap();
        assert_eq!(parsed_empty.page, None);
        assert_eq!(parsed_empty.per_page, None);
    }

    #[test]
    fn test_payout_status_deserialization() {
        use common::payout::PayoutStatus;

        assert_eq!(
            serde_json::from_str::<PayoutStatus>(r#""pending""#).unwrap(),
            PayoutStatus::Pending
        );
        assert_eq!(
            serde_json::from_str::<PayoutStatus>(r#""processing""#).unwrap(),
            PayoutStatus::Processing
        );
        assert_eq!(
            serde_json::from_str::<PayoutStatus>(r#""paid""#).unwrap(),
            PayoutStatus::Paid
        );
        assert_eq!(
            serde_json::from_str::<PayoutStatus>(r#""cancelled""#).unwrap(),
            PayoutStatus::Cancelled
        );
        assert_eq!(
            serde_json::from_str::<PayoutStatus>(r#""refunded""#).unwrap(),
            PayoutStatus::Refunded
        );
    }

    #[test]
    fn test_update_payout_status_request_serde() {
        use common::payout::{PayoutStatus, UpdatePayoutStatusRequest};

        let req = UpdatePayoutStatusRequest {
            status: PayoutStatus::Paid,
            gateway_reference: Some("GATEWAY_TX_987".to_string()),
            failure_reason: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""status":"paid""#));
        assert!(json.contains(r#""gateway_reference":"GATEWAY_TX_987""#));

        let deserialized: UpdatePayoutStatusRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, PayoutStatus::Paid);
        assert_eq!(
            deserialized.gateway_reference,
            Some("GATEWAY_TX_987".to_string())
        );
    }

    #[test]
    fn test_updated_booking_request_with_door_code() {
        use crate::apis::UpdatedBookingRequest;

        let json_data = r#"{"door_access_code":"8899"}"#;
        let parsed: UpdatedBookingRequest = serde_json::from_str(json_data).unwrap();
        assert_eq!(parsed.door_access_code.as_deref(), Some("8899"));
        assert!(parsed.status.is_none());
        assert!(parsed.metadata.is_none());
    }

    #[test]
    fn test_cron_sweep_response_serde() {
        use crate::apis::CronSweepResponse;

        let resp = CronSweepResponse {
            status: "success".to_string(),
            pre_arrival_processed: 5,
            hold_reminders_processed: 2,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""status":"success""#));
        assert!(json.contains(r#""pre_arrival_processed":5"#));
        assert!(json.contains(r#""hold_reminders_processed":2"#));
    }

    #[test]
    fn test_stay_completion_cutoff_waits_for_noon_in_jamaica() {
        use crate::apis::stay_completion_cutoff;
        use chrono::{NaiveDate, TimeZone, Utc};

        let at = |h, m| Utc.with_ymd_and_hms(2026, 11, 10, h, m, 0).unwrap();
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();

        // 11:59 Jamaica (16:59 UTC): today's checkouts are still in the villa.
        assert_eq!(stay_completion_cutoff(at(16, 59)), day(9));
        // 12:00 Jamaica (17:00 UTC): today's checkouts complete.
        assert_eq!(stay_completion_cutoff(at(17, 0)), day(10));
        // 00:30 UTC is 19:30 the previous evening in Jamaica.
        assert_eq!(stay_completion_cutoff(at(0, 30)), day(9));
        // 05:30 UTC is 00:30 Jamaica: only yesterday's checkouts.
        assert_eq!(stay_completion_cutoff(at(5, 30)), day(9));
    }

    #[test]
    fn test_statutory_tax_resolution_for_jamaican_and_fallback_countries() {
        use common::reference::SupportedCountry;

        let jm_rate = Decimal::new(15, 2);

        // Jamaican listings calculate 15% GCT
        assert_eq!(SupportedCountry::tax_rate_for("Jamaica"), jm_rate);
        assert_eq!(SupportedCountry::tax_rate_for("JM"), jm_rate);
        assert_eq!(SupportedCountry::tax_rate_for("jm"), jm_rate);

        // Unknown country code defaults safely to 15% (JM)
        assert_eq!(SupportedCountry::tax_rate_for("UnknownLand"), jm_rate);
        assert_eq!(SupportedCountry::tax_rate_for(""), jm_rate);

        // Verify end-to-end BookingCalculator with resolved tax
        let daily_rate = Decimal::new(200, 0);
        let tax_rate = SupportedCountry::tax_rate_for("Jamaica");
        let calc = BookingCalculator::new(daily_rate, 5)
            .apply_discounts(None, None)
            .apply_taxes(tax_rate)
            .finalize();

        // 5 nights @ $200 = $1,000. GCT 15% = $150. Platform fee 5% = $50. Total = $1,200.
        assert_eq!(calc.sub_total_price, Decimal::new(1000, 0));
        assert_eq!(calc.tax_rate, jm_rate);
        assert_eq!(calc.tax_value, Some(Decimal::new(15000, 2)));
        assert_eq!(calc.total_price, Decimal::new(120000, 2));
    }

    #[tokio::test]
    async fn test_get_booking_documents_and_authorization() {
        use actix_web::{App, test, web};
        use chrono::NaiveDate;
        use rust_decimal_macros::dec;
        use sqlx::types::Json;
        use sqlx_db_tester::TestPg;
        use std::env;
        use std::path::Path;
        use uuid::Uuid;

        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
        let pool = test_db.get_pool().await;

        let host_id = Uuid::now_v7();
        let guest_id = Uuid::now_v7();
        let admin_id = Uuid::now_v7();
        let stranger_id = Uuid::now_v7();

        let mut tx = pool.begin().await.unwrap();

        // 1. Create host
        let host_user = db_core::models::NewUser {
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
            roles: Some(vec![db_core::models::UserRole::Host]),
            default_currency: "USD".to_string(),
        };
        db_core::user::create_user(&mut *tx, &host_user)
            .await
            .unwrap();

        let profile = db_core::models::NewHostProfile {
            description: Some("Host".to_string()),
            verified_status: Some("pending".to_string()),
            payout_details: None,
        };
        db_core::user::create_host_profile(&mut *tx, host_id, &profile)
            .await
            .unwrap();

        // 2. Create guest
        let guest_user = db_core::models::NewUser {
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
            roles: Some(vec![db_core::models::UserRole::Booker]),
            default_currency: "USD".to_string(),
        };
        db_core::user::create_user(&mut *tx, &guest_user)
            .await
            .unwrap();

        // 3. Create admin
        let admin_user = db_core::models::NewUser {
            id: admin_id,
            email: format!("admin_{}@example.com", admin_id),
            password_hash: "hash".to_string(),
            first_name: "Admin".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![db_core::models::UserRole::Admin]),
            default_currency: "USD".to_string(),
        };
        db_core::user::create_user(&mut *tx, &admin_user)
            .await
            .unwrap();

        // 4. Create stranger
        let stranger_user = db_core::models::NewUser {
            id: stranger_id,
            email: format!("stranger_{}@example.com", stranger_id),
            password_hash: "hash".to_string(),
            first_name: "Stranger".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![db_core::models::UserRole::Booker]),
            default_currency: "USD".to_string(),
        };
        db_core::user::create_user(&mut *tx, &stranger_user)
            .await
            .unwrap();

        // 5. Create listing
        let listing = db_core::listing::create_listing(
            &mut *tx,
            &db_core::models::NewListing {
                name: format!("Doc Test Villa {}", Uuid::now_v7()),
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

        // 6. Ensure USD -> JMD exchange rate
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

        // 7. Create confirmed booking
        let booking_id = Uuid::now_v7();
        let code = format!("D{}", Uuid::now_v7().simple())[..10].to_uppercase();
        let booking = sqlx::query_as!(
            db_core::models::Booking,
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
                id, confirmation_code, guest_id, listing_id, status as "status: db_core::models::BookingStatus",
                date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<db_core::models::FeeItem>>",
                total_price, cancellation_policy as "cancellation_policy: db_core::models::CancellationPolicy",
                metadata as "metadata: Json<db_core::models::BookingMetadata>",
                door_access_code,
                cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
                created_at, updated_at
            "#,
            booking_id,
            code,
            guest_id,
            listing.id,
            NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 12, 4).unwrap(),
            Json(db_core::models::BookingMetadata {
                num_adults: 2,
                business_company_name: Some("Test Client".to_string()),
                business_tax_id: Some("TRN-111".to_string()),
                ..Default::default()
            }) as _
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();

        let supplier = db_core::models::SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        };

        // 8. Issue payment receipt
        let doc = db_core::tax_document::issue_payment_receipt(&mut tx, &booking, &supplier, None)
            .await
            .unwrap();

        tx.commit().await.unwrap();

        // Initialize actix test service
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .configure(crate::apis::configure_routes),
        )
        .await;

        let guest_jwt = api_core::auth::generate_test_jwt(guest_id);
        let host_jwt = api_core::auth::generate_test_jwt(host_id);
        let admin_jwt = api_core::auth::generate_test_jwt(admin_id);
        let stranger_jwt = api_core::auth::generate_test_jwt(stranger_id);

        // A. Guest requests documents -> 200 OK
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/bookings/{}/documents", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let body: Vec<common::tax_document::TaxDocumentResponse> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].document_number, doc.document_number);
        assert_eq!(body[0].total_amount, dec!(827.50));

        // B. Host requests documents -> 200 OK
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/bookings/{}/documents", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", host_jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        // C. Admin requests documents -> 200 OK
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/bookings/{}/documents", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", admin_jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        // D. Stranger requests documents -> 404 Not Found (auth guard)
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/bookings/{}/documents", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", stranger_jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);

        // E. No auth header -> 404 Not Found
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/bookings/{}/documents", booking.id))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);

        // F. Guest requests single document by number -> 200 OK
        let req = test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/{}/documents/{}",
                booking.id, doc.document_number
            ))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let single_doc: common::tax_document::TaxDocumentResponse =
            test::read_body_json(resp).await;
        assert_eq!(single_doc.document_number, doc.document_number);

        // G. Non-existent document number -> 404 Not Found
        let req = test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/{}/documents/NONEXISTENT",
                booking.id
            ))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);

        // H. Guest requests cancellation quote -> 200 OK without mutating DB state
        let req = test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/{}/cancellation-quote",
                booking.id
            ))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let quote: common::cancellation::RefundQuote = test::read_body_json(resp).await;
        assert_eq!(
            quote.refund_total + quote.retained_total,
            booking.total_price
        );

        // I. Stranger requests cancellation quote -> 404 Not Found
        let req = test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/{}/cancellation-quote",
                booking.id
            ))
            .insert_header(("Authorization", format!("Bearer {}", stranger_jwt)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);

        // J. Unauthenticated requests cancellation quote -> 404 Not Found
        let req = test::TestRequest::get()
            .uri(&format!(
                "/api/v1/bookings/{}/cancellation-quote",
                booking.id
            ))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);

        // Verify DB state was NOT mutated
        let doc_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM tax_document WHERE booking_id = $1",
            booking.id
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .unwrap_or(0);
        assert_eq!(doc_count, 1);

        // Clean up
        let _ = sqlx::query!("DELETE FROM tax_document WHERE booking_id = $1", booking.id)
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
            "DELETE FROM \"user\" WHERE id = $1 OR id = $2 OR id = $3 OR id = $4",
            host_id,
            guest_id,
            admin_id,
            stranger_id
        )
        .execute(&pool)
        .await;
    }

    #[actix_web::test]
    async fn test_booking_date_change_api() {
        use actix_web::{App, test, web};
        use chrono::NaiveDate;
        use rust_decimal_macros::dec;
        use sqlx::types::Json;
        use sqlx_db_tester::TestPg;
        use std::env;
        use std::path::Path;
        use uuid::Uuid;

        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        let host_id = Uuid::now_v7();
        let guest_id = Uuid::now_v7();
        let stranger_id = Uuid::now_v7();

        // 1. Host
        let host_user = db_core::models::NewUser {
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
            roles: Some(vec![db_core::models::UserRole::Host]),
            default_currency: "USD".to_string(),
        };
        db_core::user::create_user(&mut *tx, &host_user)
            .await
            .unwrap();

        let profile = db_core::models::NewHostProfile {
            description: Some("Host profile".to_string()),
            verified_status: Some("verified".to_string()),
            payout_details: None,
        };
        db_core::user::create_host_profile(&mut *tx, host_id, &profile)
            .await
            .unwrap();

        // 2. Guest
        let guest_user = db_core::models::NewUser {
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
            roles: Some(vec![db_core::models::UserRole::Booker]),
            default_currency: "USD".to_string(),
        };
        db_core::user::create_user(&mut *tx, &guest_user)
            .await
            .unwrap();

        // 3. Stranger
        let stranger_user = db_core::models::NewUser {
            id: stranger_id,
            email: format!("stranger_{}@example.com", stranger_id),
            password_hash: "hash".to_string(),
            first_name: "Stranger".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![db_core::models::UserRole::Booker]),
            default_currency: "USD".to_string(),
        };
        db_core::user::create_user(&mut *tx, &stranger_user)
            .await
            .unwrap();

        // 4. Listing
        let listing = db_core::listing::create_listing(
            &mut *tx,
            &db_core::models::NewListing {
                name: "Date Change Test Villa".to_string(),
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

        // 5. Currency exchange rate
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

        // 6. Confirmed booking Nov 1 - Nov 5
        let booking_id = Uuid::now_v7();
        let code = format!("D{}", Uuid::new_v4().simple())[..10].to_uppercase();
        let booking = sqlx::query_as!(
            db_core::models::Booking,
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
                $5, $6, 'USD', 200.00, 2, 4,
                800.00, 0.00, 127.50, '[]'::jsonb,
                1077.50, 'flexible'::cancellation_policy, $7, NULL,
                50.00, 100.00, 40.00, 0.1500, 1.000000,
                NOW(), NOW()
            )
            RETURNING
                id, confirmation_code, guest_id, listing_id, status as "status: db_core::models::BookingStatus",
                date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<db_core::models::FeeItem>>",
                total_price, cancellation_policy as "cancellation_policy: db_core::models::CancellationPolicy",
                metadata as "metadata: Json<db_core::models::BookingMetadata>",
                door_access_code,
                cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
                created_at, updated_at
            "#,
            booking_id,
            code,
            guest_id,
            listing.id,
            NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            Json(db_core::models::BookingMetadata {
                num_adults: 2,
                ..Default::default()
            }) as _
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();

        // Issue initial R1 payment receipt
        let supplier = db_core::models::SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        };
        db_core::tax_document::issue_payment_receipt(&mut tx, &booking, &supplier, None)
            .await
            .unwrap();

        // 7. Confirmed booking 2 (Nov 15 - Nov 20)
        let b2_id = Uuid::now_v7();
        let code2 = format!("E{}", Uuid::new_v4().simple())[..10].to_uppercase();
        let _booking2 = sqlx::query_as!(
            db_core::models::Booking,
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
                $5, $6, 'USD', 200.00, 2, 5,
                1000.00, 0.00, 150.00, '[]'::jsonb,
                1300.00, 'flexible'::cancellation_policy, $7, NULL,
                50.00, 100.00, 50.00, 0.1500, 1.000000,
                NOW(), NOW()
            )
            RETURNING
                id, confirmation_code, guest_id, listing_id, status as "status: db_core::models::BookingStatus",
                date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<db_core::models::FeeItem>>",
                total_price, cancellation_policy as "cancellation_policy: db_core::models::CancellationPolicy",
                metadata as "metadata: Json<db_core::models::BookingMetadata>",
                door_access_code,
                cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
                created_at, updated_at
            "#,
            b2_id,
            code2,
            guest_id,
            listing.id,
            NaiveDate::from_ymd_opt(2026, 11, 15).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(),
            Json(db_core::models::BookingMetadata {
                num_adults: 2,
                ..Default::default()
            }) as _
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();

        tx.commit().await.unwrap();

        let admin_id = Uuid::now_v7();
        let admin_user = db_core::models::NewUser {
            id: admin_id,
            email: format!("admin_{}@example.com", admin_id),
            password_hash: "hash".to_string(),
            first_name: "Admin".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![db_core::models::UserRole::Admin]),
            default_currency: "USD".to_string(),
        };
        let mut tx_admin = pool.begin().await.unwrap();
        db_core::user::create_user(&mut *tx_admin, &admin_user)
            .await
            .unwrap();
        tx_admin.commit().await.unwrap();

        use crate::settings::TaxSettings;

        let tax_settings = TaxSettings {
            taj_supplier_trn: "000-111-222".to_string(),
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
        };

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(tax_settings))
                .configure(crate::apis::configure_routes),
        )
        .await;

        let guest_jwt = api_core::auth::generate_test_jwt(guest_id);
        let host_jwt = api_core::auth::generate_test_jwt(host_id);
        let admin_jwt = api_core::auth::generate_test_jwt(admin_id);
        let stranger_jwt = api_core::auth::generate_test_jwt(stranger_id);

        // A. Stranger attempts to change dates -> 404 (IDOR guard)
        let stranger_req = test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{}", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", stranger_jwt)))
            .set_json(serde_json::json!({
                "date_from": "2026-11-01",
                "date_to": "2026-11-07"
            }))
            .to_request();
        let resp = test::call_service(&app, stranger_req).await;
        assert_eq!(resp.status(), 404);

        // A2. Non-admin Guest or Host attempting direct PATCH date/guest mutation -> 403 Forbidden
        let guest_direct_req = test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{}", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .set_json(serde_json::json!({
                "date_from": "2026-11-01",
                "date_to": "2026-11-07"
            }))
            .to_request();
        let resp = test::call_service(&app, guest_direct_req).await;
        assert_eq!(resp.status(), 403);

        let host_direct_req = test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{}", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", host_jwt)))
            .set_json(serde_json::json!({
                "number_of_persons": 3
            }))
            .to_request();
        let resp = test::call_service(&app, host_direct_req).await;
        assert_eq!(resp.status(), 403);

        // B. Admin attempts to change dates to past date -> 409 BOOKING_NOT_MODIFIABLE
        let past_req = test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{}", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", admin_jwt)))
            .set_json(serde_json::json!({
                "date_from": "2020-01-01",
                "date_to": "2020-01-05"
            }))
            .to_request();
        let resp = test::call_service(&app, past_req).await;
        assert_eq!(resp.status(), 409);
        let err_body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(err_body["code"], "BOOKING_NOT_MODIFIABLE");

        // C. Admin attempts to change dates overlapping booking 2 (Nov 16 - Nov 22) -> 409 DATE_UNAVAILABLE
        let overlap_req = test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{}", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", admin_jwt)))
            .set_json(serde_json::json!({
                "date_from": "2026-11-16",
                "date_to": "2026-11-22"
            }))
            .to_request();
        let resp = test::call_service(&app, overlap_req).await;
        assert_eq!(resp.status(), 409);
        let err_body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(err_body["code"], "DATE_UNAVAILABLE");

        // D. Admin successfully changes dates (Nov 1 - Nov 6, 5 nights) -> 200 OK
        let valid_req = test::TestRequest::patch()
            .uri(&format!("/api/v1/bookings/{}", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", admin_jwt)))
            .set_json(serde_json::json!({
                "date_from": "2026-11-01",
                "date_to": "2026-11-06"
            }))
            .to_request();
        let resp = test::call_service(&app, valid_req).await;
        assert_eq!(resp.status(), 200);

        // E. Verify revised tax receipt R2 created, supersedes R1, balance_due calculated
        let docs = db_core::tax_document::list_for_booking(&pool, booking.id)
            .await
            .unwrap();
        assert_eq!(docs.len(), 2);
        let r1 = docs.iter().find(|d| d.revision == 1).unwrap();
        assert_eq!(r1.status, db_core::models::TaxDocumentStatus::Superseded);
        let r2 = docs.iter().find(|d| d.revision == 2).unwrap();
        assert_eq!(r2.status, db_core::models::TaxDocumentStatus::Issued);
        assert_eq!(r2.document_number, format!("REC-2026-{}-R2", code));
        assert!(r2.balance_due.is_some());
        assert!(r2.amount_previously_paid.is_some());

        // F. Bilateral Reservation Change Workflow:
        // F1. Empty / no-op change quote -> 400 Bad Request
        let noop_quote_req = test::TestRequest::post()
            .uri(&format!("/api/v1/bookings/{}/change-quote", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .set_json(serde_json::json!({}))
            .to_request();
        let resp = test::call_service(&app, noop_quote_req).await;
        assert_eq!(resp.status(), 400);

        // F2. Guest previews extending stay to Nov 7 and adding 1 adult (3 adults total) -> 200 OK
        let quote_req = test::TestRequest::post()
            .uri(&format!("/api/v1/bookings/{}/change-quote", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .set_json(serde_json::json!({
                "date_from": "2026-11-01",
                "date_to": "2026-11-07",
                "num_adults": 3
            }))
            .to_request();
        let resp = test::call_service(&app, quote_req).await;
        assert_eq!(resp.status(), 200);
        let quote_body: common::models::ReservationChangeQuoteResponse =
            test::read_body_json(resp).await;
        assert_eq!(quote_body.proposed_number_of_persons, 3);
        assert!(quote_body.balance_due > rust_decimal::Decimal::ZERO);

        // F3. Guest submits change request -> 201 Created
        let create_cr_req = test::TestRequest::post()
            .uri(&format!("/api/v1/bookings/{}/change-requests", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .set_json(serde_json::json!({
                "date_from": "2026-11-01",
                "date_to": "2026-11-07",
                "num_adults": 3,
                "reason": "Bringing a friend and staying an extra night"
            }))
            .to_request();
        let resp = test::call_service(&app, create_cr_req).await;
        assert_eq!(resp.status(), 201);
        let created_cr: common::models::ReservationChangeRequestResponse =
            test::read_body_json(resp).await;
        assert_eq!(
            created_cr.status,
            common::models::ReservationChangeStatus::Pending
        );
        assert_eq!(
            created_cr.initiated_by_role,
            common::models::ChangeInitiatorRole::Guest
        );
        assert_eq!(
            created_cr.settlement_status,
            common::models::ChangeSettlementStatus::PendingCharge
        );

        // F4. Duplicate pending request -> 409 CHANGE_REQUEST_ALREADY_PENDING
        let dup_cr_req = test::TestRequest::post()
            .uri(&format!("/api/v1/bookings/{}/change-requests", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .set_json(serde_json::json!({
                "date_from": "2026-11-01",
                "date_to": "2026-11-08"
            }))
            .to_request();
        let resp = test::call_service(&app, dup_cr_req).await;
        assert_eq!(resp.status(), 409);
        let err_body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(err_body["code"], "CHANGE_REQUEST_ALREADY_PENDING");

        // F5. Guest attempts self-approval -> 403 Forbidden
        let self_approve_req = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/bookings/{}/change-requests/{}/approve",
                booking.id, created_cr.id
            ))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .set_json(serde_json::json!({}))
            .to_request();
        let resp = test::call_service(&app, self_approve_req).await;
        assert_eq!(resp.status(), 403);

        // F6. Host approves Guest's change request -> 200 OK
        let host_approve_req = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/bookings/{}/change-requests/{}/approve",
                booking.id, created_cr.id
            ))
            .insert_header(("Authorization", format!("Bearer {}", host_jwt)))
            .set_json(serde_json::json!({
                "response_note": "Approved! Looking forward to hosting your group."
            }))
            .to_request();
        let resp = test::call_service(&app, host_approve_req).await;
        assert_eq!(resp.status(), 200);
        let approved_cr: common::models::ReservationChangeRequestResponse =
            test::read_body_json(resp).await;
        assert_eq!(
            approved_cr.status,
            common::models::ReservationChangeStatus::Approved
        );
        assert_eq!(
            approved_cr.settlement_status,
            common::models::ChangeSettlementStatus::Charged
        );
        assert!(approved_cr.gateway_reference.is_some());

        // Verify booking updated to 6 nights, 3 persons, and REC-R3 issued
        let b_after = db_core::booking::get_booking_by_id(&pool, booking.id)
            .await
            .unwrap();
        assert_eq!(b_after.total_days, 6);
        assert_eq!(b_after.number_of_persons, 3);
        let docs_after_approve = db_core::tax_document::list_for_booking(&pool, booking.id)
            .await
            .unwrap();
        assert_eq!(docs_after_approve.len(), 3);
        let r3 = docs_after_approve.iter().find(|d| d.revision == 3).unwrap();
        assert_eq!(r3.status, db_core::models::TaxDocumentStatus::Issued);
        assert_eq!(r3.document_number, format!("REC-2026-{}-R3", code));

        // F7. Host initiates a shorten-stay change request (Nov 1 - Nov 5, balance_due < 0) -> Guest declines -> 200 OK
        let host_cr_req = test::TestRequest::post()
            .uri(&format!("/api/v1/bookings/{}/change-requests", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", host_jwt)))
            .set_json(serde_json::json!({
                "date_from": "2026-11-01",
                "date_to": "2026-11-05",
                "reason": "Maintenance needed on Nov 6"
            }))
            .to_request();
        let resp = test::call_service(&app, host_cr_req).await;
        assert_eq!(resp.status(), 201);
        let host_cr: common::models::ReservationChangeRequestResponse =
            test::read_body_json(resp).await;
        assert_eq!(
            host_cr.initiated_by_role,
            common::models::ChangeInitiatorRole::Host
        );
        assert_eq!(
            host_cr.settlement_status,
            common::models::ChangeSettlementStatus::PendingRefund
        );

        let guest_decline_req = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/bookings/{}/change-requests/{}/decline",
                booking.id, host_cr.id
            ))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .set_json(serde_json::json!({
                "response_note": "We already booked flights for Nov 7"
            }))
            .to_request();
        let resp = test::call_service(&app, guest_decline_req).await;
        assert_eq!(resp.status(), 200);
        let declined_cr: common::models::ReservationChangeRequestResponse =
            test::read_body_json(resp).await;
        assert_eq!(
            declined_cr.status,
            common::models::ReservationChangeStatus::Declined
        );

        // F8. Host initiates another change request and withdraws it via /cancel -> 200 OK
        let host_cr2_req = test::TestRequest::post()
            .uri(&format!("/api/v1/bookings/{}/change-requests", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", host_jwt)))
            .set_json(serde_json::json!({
                "date_from": "2026-11-01",
                "date_to": "2026-11-06"
            }))
            .to_request();
        let resp = test::call_service(&app, host_cr2_req).await;
        assert_eq!(resp.status(), 201);
        let host_cr2: common::models::ReservationChangeRequestResponse =
            test::read_body_json(resp).await;

        let host_cancel_req = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/bookings/{}/change-requests/{}/cancel",
                booking.id, host_cr2.id
            ))
            .insert_header(("Authorization", format!("Bearer {}", host_jwt)))
            .to_request();
        let resp = test::call_service(&app, host_cancel_req).await;
        assert_eq!(resp.status(), 200);
        let cancelled_cr: common::models::ReservationChangeRequestResponse =
            test::read_body_json(resp).await;
        assert_eq!(
            cancelled_cr.status,
            common::models::ReservationChangeStatus::Cancelled
        );

        // F9. List change requests returns all 3 requests
        let list_cr_req = test::TestRequest::get()
            .uri(&format!("/api/v1/bookings/{}/change-requests", booking.id))
            .insert_header(("Authorization", format!("Bearer {}", guest_jwt)))
            .to_request();
        let resp = test::call_service(&app, list_cr_req).await;
        assert_eq!(resp.status(), 200);
        let listed_crs: Vec<common::models::ReservationChangeRequestResponse> =
            test::read_body_json(resp).await;
        assert_eq!(listed_crs.len(), 3);

        // Clean up
        let _ = sqlx::query!(
            "DELETE FROM reservation_change_request WHERE booking_id = $1 OR booking_id = $2",
            booking.id,
            b2_id
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query!(
            "DELETE FROM tax_document WHERE booking_id = $1 OR booking_id = $2",
            booking.id,
            b2_id
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query!(
            "DELETE FROM host_payout_ledger WHERE booking_id = $1 OR booking_id = $2",
            booking.id,
            b2_id
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query!(
            "DELETE FROM booking_history WHERE booking_id = $1 OR booking_id = $2",
            booking.id,
            b2_id
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query!(
            "DELETE FROM booking WHERE id = $1 OR id = $2",
            booking.id,
            b2_id
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
            "DELETE FROM \"user\" WHERE id = $1 OR id = $2 OR id = $3 OR id = $4",
            host_id,
            guest_id,
            stranger_id,
            admin_id
        )
        .execute(&pool)
        .await;
    }

    #[actix_web::test]
    async fn test_complete_stays_cron() {
        use actix_web::{App, test, web};
        use chrono::Utc;
        use rust_decimal_macros::dec;
        use sqlx::types::Json;
        use sqlx_db_tester::TestPg;
        use std::env;
        use std::path::Path;
        use uuid::Uuid;

        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
        let pool = test_db.get_pool().await;

        let host_id = Uuid::now_v7();
        let guest_id = Uuid::now_v7();

        let mut tx = pool.begin().await.unwrap();

        // 1. Host
        let host_user = db_core::models::NewUser {
            id: host_id,
            email: format!("host_{}@example.com", host_id),
            password_hash: "hash".to_string(),
            first_name: "Host".to_string(),
            last_name: "Cron".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![db_core::models::UserRole::Host]),
            default_currency: "USD".to_string(),
        };
        db_core::user::create_user(&mut *tx, &host_user)
            .await
            .unwrap();

        let profile = db_core::models::NewHostProfile {
            description: Some("Host profile".to_string()),
            verified_status: Some("verified".to_string()),
            payout_details: None,
        };
        db_core::user::create_host_profile(&mut *tx, host_id, &profile)
            .await
            .unwrap();

        // 2. Guest
        let guest_user = db_core::models::NewUser {
            id: guest_id,
            email: format!("guest_{}@example.com", guest_id),
            password_hash: "hash".to_string(),
            first_name: "Guest".to_string(),
            last_name: "Cron".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![db_core::models::UserRole::Booker]),
            default_currency: "USD".to_string(),
        };
        db_core::user::create_user(&mut *tx, &guest_user)
            .await
            .unwrap();

        // 3. Listing
        let listing = db_core::listing::create_listing(
            &mut *tx,
            &db_core::models::NewListing {
                name: "Cron Stay Test Villa".to_string(),
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

        // 4. Currency FX rate
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

        // 5. Confirmed past booking (checked out in past relative to Jamaica today)
        let jamaica_today = (Utc::now() - chrono::Duration::hours(5)).date_naive();
        let check_in = jamaica_today - chrono::Duration::days(5);
        let check_out = jamaica_today - chrono::Duration::days(1);

        let booking_id = Uuid::now_v7();
        let code = format!("C{}", Uuid::new_v4().simple())[..10].to_uppercase();
        let booking = sqlx::query_as!(
            db_core::models::Booking,
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
                $5, $6, 'USD', 200.00, 2, 4,
                800.00, 0.00, 127.50, '[]'::jsonb,
                1077.50, 'flexible'::cancellation_policy, $7, NULL,
                50.00, 100.00, 40.00, 0.1500, 1.000000,
                NOW(), NOW()
            )
            RETURNING
                id, confirmation_code, guest_id, listing_id, status as "status: db_core::models::BookingStatus",
                date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                sub_total_price, discount_value, tax_value, fee_breakdown as "fee_breakdown: Json<Vec<db_core::models::FeeItem>>",
                total_price, cancellation_policy as "cancellation_policy: db_core::models::CancellationPolicy",
                metadata as "metadata: Json<db_core::models::BookingMetadata>",
                door_access_code,
                cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
                created_at, updated_at
            "#,
            booking_id,
            code,
            guest_id,
            listing.id,
            check_in,
            check_out,
            Json(db_core::models::BookingMetadata {
                num_adults: 2,
                ..Default::default()
            }) as _
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();

        // Initial R1 receipt
        let supplier = db_core::models::SupplierInfo {
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
            supplier_trn: "000-111-222".to_string(),
        };
        db_core::tax_document::issue_payment_receipt(&mut tx, &booking, &supplier, None)
            .await
            .unwrap();

        tx.commit().await.unwrap();

        let tax_settings = TaxSettings {
            taj_supplier_trn: "000-111-222".to_string(),
            supplier_name: "Our Places Jamaica Limited".to_string(),
            supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
        };
        let cron_settings = CronSettings {
            cron_secret: "test-complete-stays-secret".to_string(),
        };

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(tax_settings))
                .app_data(web::Data::new(cron_settings))
                .configure(crate::apis::configure_routes),
        )
        .await;

        // A. Missing cron secret -> 401
        let no_secret_req = test::TestRequest::patch()
            .uri("/api/v1/internal/cron/complete-stays")
            .to_request();
        let resp = test::call_service(&app, no_secret_req).await;
        assert_eq!(resp.status(), 401);

        // B. Invalid cron secrets (different lengths, same length diff char, empty) -> 401
        for bad_secret in [
            "incorrect-secret",
            "short",
            "test-complete-stays-secred", // same length, 1 char diff
            "this-is-a-much-longer-secret-that-exceeds-the-length-of-the-configured-token",
            "",
        ] {
            let bad_secret_req = test::TestRequest::patch()
                .uri("/api/v1/internal/cron/complete-stays")
                .insert_header(("x-cron-secret", bad_secret))
                .to_request();
            let resp = test::call_service(&app, bad_secret_req).await;
            assert_eq!(resp.status(), 401);
        }

        // C. Valid cron secret -> 200 OK
        let valid_req = test::TestRequest::patch()
            .uri("/api/v1/internal/cron/complete-stays")
            .insert_header(("x-cron-secret", "test-complete-stays-secret"))
            .to_request();
        let resp = test::call_service(&app, valid_req).await;
        assert_eq!(resp.status(), 200);

        let cron_resp: crate::apis::CompleteStaysResponse = test::read_body_json(resp).await;
        assert!(cron_resp.completed_count >= 1);
        assert!(cron_resp.booking_ids.contains(&booking.id));

        // D. Verify booking is now Completed
        let updated_booking = db_core::booking::get_booking_by_id(&pool, booking.id)
            .await
            .unwrap();
        assert_eq!(
            updated_booking.status,
            db_core::models::BookingStatus::Completed
        );

        // E. Verify statutory Tax Invoice issued (INV-YYYY-{code})
        let docs = db_core::tax_document::list_for_booking(&pool, booking.id)
            .await
            .unwrap();
        let invoice = docs
            .iter()
            .find(|d| d.document_type == db_core::models::TaxDocumentType::TaxInvoice)
            .expect("Tax invoice must be issued on completion");
        assert_eq!(invoice.status, db_core::models::TaxDocumentStatus::Issued);
        assert_eq!(invoice.document_number, format!("INV-2026-{}", code));
        // Security deposit strictly excluded from invoice total:
        assert_eq!(invoice.damage_deposit, Decimal::ZERO);
        // Total = 800 (subtotal) + 50 (cleaning) + 127.50 (tax) + 40 (platform) = 1017.50
        assert_eq!(invoice.total_amount, dec!(1017.50));

        // F. Idempotency: second run does not re-process or duplicate invoices
        let valid_req2 = test::TestRequest::patch()
            .uri("/api/v1/internal/cron/complete-stays")
            .insert_header(("x-cron-secret", "test-complete-stays-secret"))
            .to_request();
        let resp2 = test::call_service(&app, valid_req2).await;
        assert_eq!(resp2.status(), 200);
        let cron_resp2: crate::apis::CompleteStaysResponse = test::read_body_json(resp2).await;
        assert!(!cron_resp2.booking_ids.contains(&booking.id));

        // F. The hourly run-sweeps endpoint needs the cron secret too, and
        // reports both sweeps (the booking is already completed).
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/internal/cron/run-sweeps")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 401);
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/internal/cron/run-sweeps")
                .insert_header(("x-cron-secret", "test-complete-stays-secret"))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let sweeps: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(sweeps["notifications"]["status"], "success");
        assert!(
            !sweeps["stays"]["booking_ids"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id == &serde_json::json!(booking.id)))
        );

        let docs_after = db_core::tax_document::list_for_booking(&pool, booking.id)
            .await
            .unwrap();
        let invoice_count = docs_after
            .iter()
            .filter(|d| d.document_type == db_core::models::TaxDocumentType::TaxInvoice)
            .count();
        assert_eq!(invoice_count, 1);

        // Clean up
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

    #[test]
    fn test_booking_confirmation_guest_payload_accommodation_net_accounts_for_discounts() {
        use chrono::NaiveDate;
        use common::email::BookingConfirmationGuestPayload;
        use db_core::models::{Booking, BookingMetadata, BookingStatus, CancellationPolicy};
        use rust_decimal_macros::dec;
        use uuid::Uuid;

        let booking_id = Uuid::new_v4();
        let mut booking = Booking {
            id: booking_id,
            confirmation_code: "CONF1234".to_string(),
            guest_id: Uuid::new_v4(),
            listing_id: Uuid::new_v4(),
            status: BookingStatus::Confirmed,
            date_from: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            date_to: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            currency: "USD".to_string(),
            daily_rate: dec!(250.00),
            number_of_persons: 2,
            total_days: 4,
            sub_total_price: dec!(1000.00),
            discount_value: Some(dec!(100.00)),
            tax_value: Some(dec!(150.00)),
            fee_breakdown: sqlx::types::Json(vec![]),
            total_price: dec!(1150.00),
            cancellation_policy: CancellationPolicy::Flexible,
            metadata: sqlx::types::Json(BookingMetadata::default()),
            door_access_code: None,
            cleaning_fee: dec!(100.00),
            damage_deposit: dec!(200.00),
            platform_fee: dec!(50.00),
            tax_rate: dec!(0.15),
            exchange_rate: dec!(1.0),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Case 1: Booking with discount
        let payload_with_discount = BookingConfirmationGuestPayload {
            booking_id: booking.id,
            confirmation_code: booking.confirmation_code.clone(),
            listing_name: "Villa Azure".to_string(),
            date_from: booking.date_from.to_string(),
            date_to: booking.date_to.to_string(),
            total_price: booking.total_price,
            currency: booking.currency.clone(),
            guest_name: "Alice Smith".to_string(),
            accommodation_net: Some(booking.accommodation_net()),
            cleaning_fee: Some(booking.cleaning_fee),
            tax_rate: Some(booking.tax_rate),
            tax_amount: booking.tax_value,
            platform_fee: Some(booking.platform_fee),
            damage_deposit: Some(booking.damage_deposit),
            receipt_url: Some("http://localhost:8080/bookings/doc/1".to_string()),
        };

        assert_eq!(
            payload_with_discount.accommodation_net,
            Some(dec!(900.00)),
            "accommodation_net must be subtotal minus discount (1000 - 100 = 900)"
        );

        // Case 2: Booking without discount (None)
        booking.discount_value = None;
        let payload_no_discount = BookingConfirmationGuestPayload {
            booking_id: booking.id,
            confirmation_code: booking.confirmation_code.clone(),
            listing_name: "Villa Azure".to_string(),
            date_from: booking.date_from.to_string(),
            date_to: booking.date_to.to_string(),
            total_price: booking.total_price,
            currency: booking.currency.clone(),
            guest_name: "Alice Smith".to_string(),
            accommodation_net: Some(booking.accommodation_net()),
            cleaning_fee: Some(booking.cleaning_fee),
            tax_rate: Some(booking.tax_rate),
            tax_amount: booking.tax_value,
            platform_fee: Some(booking.platform_fee),
            damage_deposit: Some(booking.damage_deposit),
            receipt_url: Some("http://localhost:8080/bookings/doc/1".to_string()),
        };

        assert_eq!(
            payload_no_discount.accommodation_net,
            Some(dec!(1000.00)),
            "accommodation_net without discount must equal subtotal"
        );
    }

    #[tokio::test]
    async fn test_unavailable_ranges_endpoint() {
        use actix_web::{App, test, web};
        use chrono::NaiveDate;
        use rust_decimal::Decimal;
        use rust_decimal_macros::dec;
        use sqlx_db_tester::TestPg;
        use std::path::Path;
        use uuid::Uuid;

        dotenvy::dotenv().ok();
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
        let pool = test_db.get_pool().await;

        let new_user = |id: Uuid, role: db_core::models::UserRole| db_core::models::NewUser {
            id,
            email: format!("u_{id}@example.com"),
            password_hash: "hash".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![role]),
            default_currency: "USD".to_string(),
        };

        let host_id = Uuid::now_v7();
        db_core::user::create_user(&pool, &new_user(host_id, db_core::models::UserRole::Host))
            .await
            .unwrap();
        db_core::user::create_host_profile(
            &pool,
            host_id,
            &db_core::models::NewHostProfile {
                description: None,
                verified_status: Some("pending".to_string()),
                payout_details: None,
            },
        )
        .await
        .unwrap();
        let listing = db_core::listing::create_listing(
            &pool,
            &db_core::models::NewListing {
                name: format!("Villa {}", Uuid::now_v7()),
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
            },
        )
        .await
        .unwrap();

        let day = |m: u32, d: u32| NaiveDate::from_ymd_opt(2030, m, d).unwrap();
        // Back-to-back stays (merged in the response) and one separate stay.
        for (from, to) in [
            (day(5, 1), day(5, 4)),
            (day(5, 4), day(5, 6)),
            (day(5, 20), day(5, 22)),
        ] {
            let guest_id = Uuid::now_v7();
            db_core::user::create_user(
                &pool,
                &new_user(guest_id, db_core::models::UserRole::Booker),
            )
            .await
            .unwrap();
            db_core::booking::create_booking(
                &pool,
                &db_core::models::NewBooking {
                    confirmation_code: Uuid::now_v7().simple().to_string()[22..].to_uppercase(),
                    guest_id,
                    listing_id: listing.id,
                    date_from: from,
                    date_to: to,
                    currency: "USD".to_string(),
                    daily_rate: dec!(200.00),
                    number_of_persons: 2,
                    total_days: (to - from).num_days() as i32,
                    sub_total_price: dec!(400.00),
                    discount_value: None,
                    tax_value: None,
                    fee_breakdown: vec![],
                    total_price: dec!(400.00),
                    cancellation_policy: db_core::models::CancellationPolicy::Flexible,
                    metadata: db_core::models::BookingMetadata {
                        num_adults: 2,
                        ..Default::default()
                    },
                    door_access_code: None,
                    cleaning_fee: Decimal::ZERO,
                    damage_deposit: Decimal::ZERO,
                    platform_fee: Decimal::ZERO,
                    tax_rate: dec!(0.1000),
                    exchange_rate: Decimal::ONE,
                },
            )
            .await
            .unwrap();
        }

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .configure(crate::apis::configure_routes),
        )
        .await;
        let get = |uri: String| test::TestRequest::get().uri(&uri).to_request();
        let base = format!("/api/v1/bookings/listing/{}/unavailable-ranges", listing.id);

        // Public, merged, sorted, and dates only.
        let resp =
            test::call_service(&app, get(format!("{base}?from=2030-05-01&to=2030-06-01"))).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(
            body,
            serde_json::json!({"ranges": [
                {"date_from": "2030-05-01", "date_to": "2030-05-06"},
                {"date_from": "2030-05-20", "date_to": "2030-05-22"}
            ]})
        );

        // Bad windows -> 400.
        for query in [
            "from=2030-05-10&to=2030-05-10",
            "from=2030-01-01&to=2031-01-03",
        ] {
            let resp = test::call_service(&app, get(format!("{base}?{query}"))).await;
            assert_eq!(resp.status(), 400, "{query}");
        }

        // Unknown listing -> 404.
        let resp = test::call_service(
            &app,
            get(format!(
                "/api/v1/bookings/listing/{}/unavailable-ranges?from=2030-05-01&to=2030-06-01",
                Uuid::now_v7()
            )),
        )
        .await;
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn test_admin_exchange_rates_endpoint() {
        use actix_web::{App, test, web};
        use rust_decimal_macros::dec;
        use sqlx_db_tester::TestPg;
        use std::path::Path;
        use uuid::Uuid;

        dotenvy::dotenv().ok();
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        let test_db = TestPg::new(db_url, Path::new("../../db_core/migrations"));
        let pool = test_db.get_pool().await;

        let new_user = |id: Uuid, role: db_core::models::UserRole| db_core::models::NewUser {
            id,
            email: format!("u_{id}@example.com"),
            password_hash: "hash".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: Some(vec![role]),
            default_currency: "USD".to_string(),
        };
        let (admin_id, host_id, booker_id) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        db_core::user::create_user(&pool, &new_user(admin_id, db_core::models::UserRole::Admin))
            .await
            .unwrap();
        db_core::user::create_user(&pool, &new_user(host_id, db_core::models::UserRole::Host))
            .await
            .unwrap();
        db_core::user::create_user(
            &pool,
            &new_user(booker_id, db_core::models::UserRole::Booker),
        )
        .await
        .unwrap();

        // A newer USD/JMD rate supersedes the migration's seed.
        sqlx::query(
            "INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
             VALUES ('USD', 'JMD', 157.250000, NOW() + INTERVAL '1 second')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .configure(crate::apis::configure_routes),
        )
        .await;

        let get = |jwt: String| {
            test::TestRequest::get()
                .uri("/api/v1/admin/exchange-rates")
                .insert_header(("Authorization", format!("Bearer {jwt}")))
                .to_request()
        };

        let resp = test::call_service(&app, get(api_core::auth::generate_test_jwt(admin_id))).await;
        assert_eq!(resp.status(), 200);
        let body: common::exchange_rate::ExchangeRatesResponse = test::read_body_json(resp).await;
        let usd_jmd: Vec<_> = body
            .rates
            .iter()
            .filter(|r| r.base_currency == "USD" && r.target_currency == "JMD")
            .collect();
        assert_eq!(usd_jmd.len(), 1, "one row per pair");
        assert_eq!(usd_jmd[0].rate, dec!(157.25));

        // Hosts get the same read-only view.
        let resp = test::call_service(&app, get(api_core::auth::generate_test_jwt(host_id))).await;
        assert_eq!(resp.status(), 200);

        let resp =
            test::call_service(&app, get(api_core::auth::generate_test_jwt(booker_id))).await;
        assert_eq!(resp.status(), 401);
    }
}
