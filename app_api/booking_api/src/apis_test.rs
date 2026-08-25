#[cfg(test)]
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

        // Discounted -> Taxed (10% tax)
        let taxed = discounted.apply_taxes();
        assert_eq!(taxed.tax_value.unwrap(), Decimal::new(90, 0));

        // Taxed -> Final
        let final_calc = taxed.finalize();
        // Platform fee is 5% of discounted subtotal (900 * 0.05 = 45)
        assert_eq!(final_calc.fee_breakdown.len(), 1);
        assert_eq!(final_calc.fee_breakdown[0].amount, Decimal::new(45, 0));

        // Total price = 900 (subtotal) + 90 (tax) + 45 (fee) = 1035
        assert_eq!(final_calc.total_price, Decimal::new(1035, 0));
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
            .apply_taxes()
            .finalize();

        assert_eq!(calc.sub_total_price, Decimal::new(1450, 0));
        assert_eq!(calc.tax_value, Some(Decimal::new(14500, 2))); // $145.00
        assert_eq!(calc.fee_breakdown[0].amount, Decimal::new(7250, 2)); // $72.50
        assert_eq!(calc.total_price, Decimal::new(166750, 2)); // 1450 + 145 + 72.50 = 1667.50
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
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let res = map_booking_to_response(booking);
        assert_eq!(res.confirmation_code, "CONF-TEST123");
        assert_eq!(res.status, "Confirmed");
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
}
