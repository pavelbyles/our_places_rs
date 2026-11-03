//! A created hold costs what the golden file says, which is also what
//! `listing_api`'s search totals are held to.

use super::authz_tests::{settings, tax_settings, user};
use super::stay_pricing_pin_tests::{amount, golden_cases, seed_case, test_db};
use actix_web::{App, test, web};
use db_core::models::UserRole;
use uuid::Uuid;

#[tokio::test]
async fn a_hold_costs_what_every_golden_case_says() {
    for case in golden_cases() {
        let name = case["name"].as_str().expect("name");
        let test_db = test_db();
        let pool = test_db.get_pool().await;
        let villa = seed_case(&pool, &case).await;
        let guest = user(&pool, UserRole::Booker).await;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(settings()))
                .app_data(web::Data::new(tax_settings()))
                .configure(crate::apis::configure_routes),
        )
        .await;

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/bookings")
                .insert_header(("Accept", "application/json"))
                .insert_header((
                    "Authorization",
                    format!("Bearer {}", api_core::auth::generate_test_jwt(guest)),
                ))
                .set_json(serde_json::json!({
                    "guest_id": guest,
                    "listing_id": villa,
                    "check_in": case["check_in"],
                    "check_out": case["check_out"],
                    "num_adults": case["guests"],
                    "num_children": 0,
                    "num_infants": 0,
                    "num_pets": 0,
                    "is_business_trip": false,
                    "currency": case["currency"],
                    "agreed_cancellation_policy": "Flexible",
                }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 201, "{name}");

        let id: Uuid = sqlx::query_scalar("SELECT id FROM booking WHERE guest_id = $1")
            .bind(guest)
            .fetch_one(&pool)
            .await
            .expect("the hold");
        let booking = db_core::booking::get_booking_by_id(&pool, id)
            .await
            .expect("read the hold");
        assert_eq!(
            booking.total_price,
            amount(&case["expected"]["total"]),
            "{name}"
        );
    }
}
