#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::apis::BookingCalculator;
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
}
