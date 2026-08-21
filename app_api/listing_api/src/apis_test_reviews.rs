
#[actix_web::test]
async fn test_reviews_endpoints() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;
    let mut conn = pool.acquire().await.unwrap();

    let user_id = create_test_user(&mut *conn).await;
    let guest_id = create_test_user(&mut *conn).await;

    // 1. Create a listing
    let new_listing = NewListing {
        name: "Test Review Listing".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: Some(dec!(100.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
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
    };
    let listing = db_core::listing::create_listing(&mut *conn, &new_listing)
        .await
        .unwrap();

    // 2. Create a booking to attach the review to
    let booking_id = Uuid::now_v7();
    let conf_code = format!("TEST{}", booking_id.simple().to_string().chars().take(8).collect::<String>());
    sqlx::query!(
        "INSERT INTO booking (id, confirmation_code, guest_id, listing_id, date_from, date_to, daily_rate, number_of_persons, total_days, sub_total_price, total_price, cancellation_policy, status) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'flexible', 'completed')",
        booking_id,
        conf_code,
        guest_id,
        listing.id,
        Utc::now().date_naive() - chrono::Duration::days(10),
        Utc::now().date_naive() - chrono::Duration::days(5),
        dec!(100.00),
        2,
        5,
        dec!(500.00),
        dec!(500.00)
    )
    .execute(&mut *conn)
    .await
    .unwrap();

    // 3. Generate a token
    let token = db_core::review::create_review_token(&mut *conn, booking_id).await.unwrap();

    // Make the token valid immediately for the test
    sqlx::query!(
        "UPDATE review_token SET valid_from = $1 WHERE id = $2",
        Utc::now() - chrono::Duration::hours(1),
        token.id
    )
    .execute(&mut *conn)
    .await
    .unwrap();

    // Initialize the API App
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes)
            .configure(crate::apis::configure_routes),
    )
    .await;

    // --- TEST 1: GET Token Info ---
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/reviews/token/{}", token.token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    if !status.is_success() {
        let body = test::read_body(resp).await;
        panic!("TEST 1 failed: {:?} - {:?}", status, body);
    }
    let info: common::models::ReviewTokenInfoResponse = test::read_body_json(resp).await;
    assert!(info.is_valid);
    assert_eq!(info.listing_name, "Test Review Listing");

    // --- TEST 2: POST Submit Review ---
    let submit_req = common::models::NewReviewRequest {
        token: token.token.clone(),
        cleanliness_rating: 5,
        accuracy_rating: 4,
        location_rating: 5,
        value_rating: 4,
        public_review_text: Some("Great stay!".to_string()),
        private_host_feedback: None,
    };

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/reviews/token/{}", token.token))
        .set_json(&submit_req)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    if !status.is_success() {
        let body = test::read_body(resp).await;
        panic!("TEST 2 failed: {:?} - {:?}", status, body);
    }

    // --- TEST 3: GET Listing Reviews ---
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/listings/{}/reviews", listing.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    if !status.is_success() {
        let body = test::read_body(resp).await;
        panic!("TEST 3 failed: {:?} - {:?}", status, body);
    }
    let reviews: Vec<common::models::ReviewResponse> = test::read_body_json(resp).await;
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].overall_rating, 4.5);
    
    // --- TEST 4: GET Listing Details with Summary ---
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/listings/{}", listing.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    if !status.is_success() {
        let body = test::read_body(resp).await;
        panic!("TEST 4 failed: {:?} - {:?}", status, body);
    }
    let listing_details: common::models::ListingDetails = test::read_body_json(resp).await;
    assert!(listing_details.rating_summary.is_some());
    assert_eq!(listing_details.rating_summary.unwrap().review_count, 1);

    // --- TEST 5: POST Host Reply ---
    let reply_req = common::models::HostReplyRequest {
        reply_text: "Thank you!".to_string(),
    };
    
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/reviews/{}/reply", reviews[0].id))
        .insert_header(("x-user-id", user_id.to_string()))
        .set_json(&reply_req)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    if !status.is_success() {
        let body = test::read_body(resp).await;
        panic!("TEST 5 failed: {:?} - {:?}", status, body);
    }
}

#[actix_web::test]
async fn test_booking_review_token_eligibility_and_lifecycle() {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let migrations_path = Path::new("../../db_core/migrations");
    let test_db = TestPg::new(db_url, migrations_path);
    let pool = test_db.get_pool().await;
    let mut conn = pool.acquire().await.unwrap();

    let user_id = create_test_user(&mut *conn).await;
    let guest_id = create_test_user(&mut *conn).await;

    // 1. Create a listing
    let new_listing = NewListing {
        name: "Eligibility Test Listing".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(150.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 2,
        bedrooms: 1,
        beds: 1,
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
    };
    let listing = db_core::listing::create_listing(&mut *conn, &new_listing)
        .await
        .unwrap();

    // 2. Create a concluded booking (checkout 5 days ago, well within 15 days)
    let booking_id = Uuid::now_v7();
    let conf_code = format!(
        "TEST{}",
        booking_id
            .simple()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>()
    );
    sqlx::query!(
        "INSERT INTO booking (id, confirmation_code, guest_id, listing_id, date_from, date_to, daily_rate, number_of_persons, total_days, sub_total_price, total_price, cancellation_policy, status) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'flexible', 'confirmed')",
        booking_id,
        conf_code,
        guest_id,
        listing.id,
        Utc::now().date_naive() - chrono::Duration::days(9),
        Utc::now().date_naive() - chrono::Duration::days(5),
        dec!(150.00),
        2,
        4,
        dec!(600.00),
        dec!(600.00)
    )
    .execute(&mut *conn)
    .await
    .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(get_test_settings()))
            .configure(configure_routes)
            .configure(crate::apis::configure_routes),
    )
    .await;

    // 3. Request booking review token via API
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/reviews/booking/{}/token", booking_id))
        .insert_header(("x-user-id", guest_id.to_string()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let eligibility: common::models::BookingReviewEligibility = test::read_body_json(resp).await;
    assert!(eligibility.is_eligible);
    assert!(!eligibility.has_reviewed);
    assert_eq!(eligibility.days_remaining, Some(10));
    assert!(eligibility.token.is_some());

    let token_str = eligibility.token.unwrap();

    // 4. Submit review with the acquired token
    let submit_req = common::models::NewReviewRequest {
        token: token_str.clone(),
        cleanliness_rating: 5,
        accuracy_rating: 5,
        location_rating: 5,
        value_rating: 5,
        public_review_text: Some("Outstanding Jamaican villa experience!".to_string()),
        private_host_feedback: None,
    };

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/reviews/token/{}", token_str))
        .set_json(&submit_req)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // 5. Query token eligibility again - should indicate already reviewed
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/reviews/booking/{}/token", booking_id))
        .insert_header(("x-user-id", guest_id.to_string()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let eligibility_after: common::models::BookingReviewEligibility =
        test::read_body_json(resp).await;
    assert!(!eligibility_after.is_eligible);
    assert!(eligibility_after.has_reviewed);
}
