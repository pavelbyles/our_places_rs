use super::{
    create_listing, create_listing_image_presigns, delete_listing_image, get_listing_by_id,
    get_listings, update_listing_image_to_processing, update_listing_with_conn,
};
use crate::error::DbError;
use crate::models::{NewHostProfile, NewListing, NewUser, UpdatedListing};
use crate::user::{create_host_profile, create_user};
use rust_decimal_macros::dec;
use sqlx::{Connection, PgConnection};
use std::collections::HashSet;
use std::env;
use uuid::Uuid;

/// Helper function to establish a fresh connection to the test database.
async fn setup_test_db() -> PgConnection {
    dotenvy::dotenv().ok();
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string());

    PgConnection::connect(&database_url)
        .await
        .expect("Failed to connect to Postgres")
}

async fn create_test_user_no_profile(conn: &mut PgConnection) -> Uuid {
    let id = Uuid::now_v7();
    let new_user = NewUser {
        id,
        email: format!("test_{}@example.com", id),
        password_hash: "secret".to_string(),
        first_name: "Test".to_string(),
        last_name: "User".to_string(),
        phone_number: None,
        is_active: true,
        is_verified: true,
        verification_code: None,
        verification_code_expires_at: None,
        attributes: serde_json::json!({}),
        roles: None,
        default_currency: "USD".to_string(),
    };
    create_user(&mut *conn, &new_user)
        .await
        .expect("Failed to create test user");
    id
}

async fn create_test_user_with_host_profile(conn: &mut PgConnection) -> Uuid {
    let user_id = create_test_user_no_profile(&mut *conn).await;
    let profile = NewHostProfile {
        description: Some("Test Host".to_string()),
        verified_status: Some("pending".to_string()),
        payout_details: None,
    };
    create_host_profile(&mut *conn, user_id, &profile)
        .await
        .expect("Failed to create host profile");
    user_id
}

#[tokio::test]
async fn test_create_listing() {
    let mut conn = setup_test_db().await;
    let mut tx = conn.begin().await.expect("Failed to begin transaction");

    let user_id = create_test_user_with_host_profile(&mut tx).await;
    let new_listing = NewListing {
        name: "Cozy Test Cottage".to_string(),
        user_id,
        description: Some("A beautiful cottage for testing.".to_string()),
        listing_structure_id: 2, // House
        country: "Testland".to_string(),
        price_per_night: Some(dec!(150.75)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 4,
        bedrooms: 2,
        beds: 2,
        full_bathrooms: 1,
        half_bathrooms: 0,
        square_meters: Some(100),
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };

    let created_listing = create_listing(&mut *tx, &new_listing).await.unwrap();

    assert_eq!(created_listing.name, new_listing.name);
    assert!(!created_listing.id.is_nil());
    assert!(created_listing.is_active);
}

#[tokio::test]
async fn test_create_listing_fails_without_host_profile() {
    let mut conn = setup_test_db().await;
    let mut tx = conn.begin().await.expect("Failed to begin transaction");

    let user_id = create_test_user_no_profile(&mut tx).await;
    let new_listing = NewListing {
        name: "Fail listing".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: None,
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
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };

    let result = create_listing(&mut *tx, &new_listing).await;
    assert!(matches!(
        result,
        Err(DbError::ValidationError(msg)) if msg == "User must have a host profile to create a listing"
    ));
}

#[tokio::test]
async fn test_get_listing_by_id() {
    let mut conn = setup_test_db().await;
    let mut tx = conn.begin().await.expect("Failed to begin transaction");

    let user_id = create_test_user_with_host_profile(&mut tx).await;
    let new_listing = NewListing {
        name: "Fetch Me Listing".to_string(),
        user_id,
        description: None,
        listing_structure_id: 3,
        country: "Republic of Testing".to_string(),
        price_per_night: Some(dec!(99.99)),
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
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    let created_listing = create_listing(&mut *tx, &new_listing).await.unwrap();

    let fetched_listing = get_listing_by_id(&mut *tx, created_listing.id)
        .await
        .unwrap();

    assert_eq!(created_listing.id, fetched_listing.listing.id);

    let non_existent_id = Uuid::now_v7();
    let result = get_listing_by_id(&mut *tx, non_existent_id).await;
    assert!(matches!(
        result,
        Err(DbError::Sqlx(sqlx::Error::RowNotFound))
    ));
}

#[tokio::test]
async fn test_get_listings_pagination() {
    let mut conn = setup_test_db().await;
    let mut tx = conn.begin().await.expect("Failed to begin transaction");

    // Create 3 listings. We will order them by name for deterministic testing.
    let user_id = create_test_user_with_host_profile(&mut tx).await;
    let mut created_ids = Vec::new();
    for i in 1..=3 {
        let listing = NewListing {
            name: format!("Pagination Test Listing {}", i),
            user_id,
            description: None,
            listing_structure_id: 1,
            country: "Testland".to_string(),
            price_per_night: None,
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
            commission_pct: Some(dec!(0.0000)),
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: None,
            extra_guest_fee: None,
            is_active: true,
        };
        let created = create_listing(&mut *tx, &listing).await.unwrap();
        created_ids.push(created.id);
    }
    // Since we sort by added_at DESC, id DESC, and all have same added_at (transaction time),
    // we need to know the order of IDs to predict page content.
    // However, instead of predicting order, we can check that standard pagination properties hold
    // AND that our items are present in the top results (since they are newest).

    // Page 1 (Limit 2)
    let page1 = get_listings(&mut *tx, 1, 2, None).await.unwrap();
    assert_eq!(page1.len(), 2);

    // Page 2 (Limit 2)
    let page2 = get_listings(&mut *tx, 2, 2, None).await.unwrap();
    // Page 2 might have >1 items if DB had pre-existing data.
    // But we expect at least 1 of ours.

    // Check that our created items are in the first 3 results (across p1 and p2)
    let mut top_3_ids = Vec::new();
    top_3_ids.extend(page1.iter().map(|l| l.id));
    if let Some(l) = page2.first() {
        top_3_ids.push(l.id);
    }

    // Verify all created_ids are in the found top results.
    // Note: Newest items should come first.
    let created_set: HashSet<Uuid> = created_ids.into_iter().collect();
    let found_set: HashSet<Uuid> = top_3_ids.into_iter().collect();

    // We assert that all our created items are found.
    for id in created_set {
        assert!(
            found_set.contains(&id),
            "Created listing {} not found in top pagination results",
            id
        );
    }
}

#[tokio::test]
async fn test_get_listings_uniqueness_across_pages() {
    let mut conn = setup_test_db().await;
    let mut tx = conn.begin().await.expect("Failed to begin transaction");

    const TOTAL_RECORDS: i32 = 7;
    const PER_PAGE: u32 = 3;

    let user_id = create_test_user_with_host_profile(&mut tx).await;
    for i in 1..=TOTAL_RECORDS {
        let listing = NewListing {
            name: format!("Uniqueness Test Listing {}", i),
            user_id,
            description: None,
            listing_structure_id: 1,
            country: "Testland".to_string(),
            price_per_night: None,
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
            commission_pct: Some(dec!(0.0000)),
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: None,
            extra_guest_fee: None,
            is_active: true,
        };
        create_listing(&mut *tx, &listing).await.unwrap();
    }

    let mut all_fetched_listings = Vec::new();
    let mut current_page = 1;
    loop {
        let page_results = get_listings(&mut *tx, current_page, PER_PAGE, None)
            .await
            .unwrap();
        if page_results.is_empty() {
            break;
        }
        all_fetched_listings.extend(page_results);
        current_page += 1;
    }

    assert!(
        all_fetched_listings.len() as i32 >= TOTAL_RECORDS,
        "Pagination should return at least the inserted number of listings"
    );

    let ids: Vec<Uuid> = all_fetched_listings.iter().map(|l| l.id).collect();
    let unique_ids: HashSet<Uuid> = ids.iter().cloned().collect();

    assert_eq!(
        ids.len(),
        unique_ids.len(),
        "Found duplicate listings across different pages"
    );
}

#[tokio::test]
async fn test_get_listings_filtering() {
    let mut conn = setup_test_db().await;
    let mut tx = conn.begin().await.expect("Failed to begin transaction");

    let user_id = create_test_user_with_host_profile(&mut tx).await;

    // 1. Apartment in Jamaica, cheap
    let listing1 = NewListing {
        name: "Cheap Apartment Jamaica".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1, // Apartment
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(50.00)),
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
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    create_listing(&mut *tx, &listing1).await.unwrap();

    // 2. Villa in Jamaica, expensive
    let listing2 = NewListing {
        name: "Luxury Villa Jamaica".to_string(),
        user_id,
        description: None,
        listing_structure_id: 5, // Villa
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(500.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 6,
        bedrooms: 3,
        beds: 3,
        full_bathrooms: 3,
        half_bathrooms: 1,
        square_meters: None,
        latitude: None,
        longitude: None,
        listing_details: None,
        city: None,
        base_currency: "USD".to_string(),
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    create_listing(&mut *tx, &listing2).await.unwrap();

    // 3. Apartment in USA, cheap
    let listing3 = NewListing {
        name: "Cheap Apartment USA".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1, // Apartment
        country: "USA".to_string(),
        price_per_night: Some(dec!(60.00)),
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
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    create_listing(&mut *tx, &listing3).await.unwrap();

    // Test Filter by Country (Jamaica)
    let filter_jamaica = common::models::ListingFilter {
        name: None,
        country: Some("Jamaica".to_string()),
        min_price: None,
        max_price: None,
        structure_type: vec![],
        owner: None,
        resolution: None,
        currency: None,
        user_id: None,
        host: None,
        include_inactive: false,
        ..Default::default()
    };
    let results = get_listings(&mut *tx, 1, 10, Some(filter_jamaica))
        .await
        .unwrap();
    // Should find listing1 and listing2 (we might have other data, but these surely)
    // We filter results locally to verify *our* created ones are there only if they match
    let found_names: Vec<String> = results.iter().map(|l| l.name.clone()).collect();
    assert!(found_names.contains(&"Cheap Apartment Jamaica".to_string()));
    assert!(found_names.contains(&"Luxury Villa Jamaica".to_string()));
    assert!(!found_names.contains(&"Cheap Apartment USA".to_string()));

    // Test Filter by Price (< $100)
    let filter_price = common::models::ListingFilter {
        name: None,
        country: None,
        min_price: None,
        max_price: Some(dec!(100.00)),
        structure_type: vec![],
        owner: None,
        resolution: None,
        currency: None,
        user_id: None,
        host: None,
        include_inactive: false,
        ..Default::default()
    };
    let results = get_listings(&mut *tx, 1, 10, Some(filter_price))
        .await
        .expect("Failed to fetch listings");

    let found_names_price: Vec<String> = results.iter().map(|l| l.name.clone()).collect();
    assert!(found_names_price.contains(&"Cheap Apartment Jamaica".to_string()));
    assert!(found_names_price.contains(&"Cheap Apartment USA".to_string()));

    // 2. Filter by Structure Type (Villa)
    let filter_villa = common::models::ListingFilter {
        name: None,
        country: None,
        min_price: None,
        max_price: None,
        structure_type: vec!["Villa".to_string()],
        owner: None,
        resolution: None,
        currency: None,
        user_id: None,
        host: None,
        include_inactive: false,
        ..Default::default()
    };
    let results = get_listings(&mut *tx, 1, 10, Some(filter_villa))
        .await
        .unwrap();
    let found_names: Vec<String> = results.iter().map(|l| l.name.clone()).collect();
    assert!(found_names.contains(&"Luxury Villa Jamaica".to_string()));
    assert!(!found_names.contains(&"Cheap Apartment Jamaica".to_string()));
}

#[tokio::test]
async fn test_update_listing_image_to_processing() {
    let mut conn = setup_test_db().await;
    let mut tx = conn.begin().await.expect("Failed to begin transaction");

    let user_id = create_test_user_with_host_profile(&mut tx).await;
    let listing = NewListing {
        name: "Test Image Processing Listing".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: None,
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
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    let created_listing = create_listing(&mut *tx, &listing).await.unwrap();

    let image_md = common::models::PendingImageMetadata {
        client_file_id: "test-file-123".to_string(),
        content_type: "image/jpeg".to_string(),
        size_bytes: 1024,
        display_order: 0,
    };
    let images = vec![(Uuid::now_v7(), image_md, "https://upload.url".to_string())];
    let presigns = create_listing_image_presigns(&mut *tx, created_listing.id, &images)
        .await
        .unwrap();

    let image_id = presigns[0].id;

    let result =
        update_listing_image_to_processing(&mut *tx, image_id, 2048, "image/webp".to_string())
            .await;

    match result {
        Ok(_) => println!("Successfully updated image"),
        Err(e) => panic!("Database error occurred: {:?}", e),
    }
}

#[tokio::test]
async fn test_delete_listing_image_promotes_next_primary() {
    let mut conn = setup_test_db().await;
    let mut tx = conn.begin().await.expect("Failed to begin transaction");

    let user_id = create_test_user_with_host_profile(&mut tx).await;
    let listing = NewListing {
        name: "Test Image Delete Listing".to_string(),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Testland".to_string(),
        price_per_night: None,
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
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    let created_listing = create_listing(&mut *tx, &listing).await.unwrap();
    let images: Vec<_> = (0..2)
        .map(|i| {
            let md = common::models::PendingImageMetadata {
                client_file_id: i.to_string(),
                content_type: "image/jpeg".to_string(),
                size_bytes: 1024,
                display_order: i,
            };
            (Uuid::now_v7(), md, "https://upload.url".to_string())
        })
        .collect();
    let presigns = create_listing_image_presigns(&mut *tx, created_listing.id, &images)
        .await
        .unwrap();
    let (first, second) = (presigns[0].id, presigns[1].id);

    // Another listing's id never matches.
    let wrong = delete_listing_image(&mut tx, Uuid::now_v7(), first).await;
    assert!(matches!(
        wrong,
        Err(DbError::Sqlx(sqlx::Error::RowNotFound))
    ));

    delete_listing_image(&mut tx, created_listing.id, first)
        .await
        .unwrap();
    let primary: bool = sqlx::query_scalar("SELECT is_primary FROM listing_image WHERE id = $1")
        .bind(second)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(primary);
}

#[tokio::test]
async fn test_get_listings_owner_exact_email_filtering() {
    let mut conn = setup_test_db().await;
    let mut tx = conn.begin().await.expect("Failed to begin transaction");

    // Create User 1: host_1@example.com
    let id1 = Uuid::now_v7();
    let email1 = format!("host_{}@example.com", id1);
    let u1 = crate::user::create_user(
        &mut *tx,
        &crate::models::NewUser {
            id: id1,
            email: email1.clone(),
            password_hash: "hash123".to_string(),
            first_name: "Host".to_string(),
            last_name: "One".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: None,
            default_currency: "USD".to_string(),
        },
    )
    .await
    .unwrap();

    crate::user::create_host_profile(
        &mut *tx,
        u1.id,
        &crate::models::NewHostProfile {
            verified_status: Some("verified".to_string()),
            payout_details: None,
            description: None,
        },
    )
    .await
    .unwrap();

    // Create User 2: other_host_1@example.com
    let id2 = Uuid::now_v7();
    let email2 = format!("other_{}", email1);
    let u2 = crate::user::create_user(
        &mut *tx,
        &crate::models::NewUser {
            id: id2,
            email: email2.clone(),
            password_hash: "hash123".to_string(),
            first_name: "Host".to_string(),
            last_name: "Two".to_string(),
            phone_number: None,
            is_active: true,
            is_verified: true,
            verification_code: None,
            verification_code_expires_at: None,
            attributes: serde_json::json!({}),
            roles: None,
            default_currency: "USD".to_string(),
        },
    )
    .await
    .unwrap();

    crate::user::create_host_profile(
        &mut *tx,
        u2.id,
        &crate::models::NewHostProfile {
            verified_status: Some("verified".to_string()),
            payout_details: None,
            description: None,
        },
    )
    .await
    .unwrap();

    // Create Listing for User 1
    let l1 = NewListing {
        name: "Host 1 Listing".to_string(),
        user_id: u1.id,
        description: None,
        listing_structure_id: 1,
        country: "JM".to_string(),
        price_per_night: None,
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
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    create_listing(&mut *tx, &l1).await.unwrap();

    // Create Listing for User 2
    let l2 = NewListing {
        name: "Host 2 Listing".to_string(),
        user_id: u2.id,
        description: None,
        listing_structure_id: 1,
        country: "JM".to_string(),
        price_per_night: None,
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
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    create_listing(&mut *tx, &l2).await.unwrap();

    // Query with exact email for User 1
    let filter1 = common::models::ListingFilter {
        name: None,
        country: None,
        min_price: None,
        max_price: None,
        structure_type: vec![],
        owner: Some(u1.email.clone()),
        resolution: None,
        currency: None,
        user_id: None,
        host: None,
        include_inactive: false,
        ..Default::default()
    };
    let results1 = get_listings(&mut *tx, 1, 10, Some(filter1)).await.unwrap();
    assert_eq!(results1.len(), 1);
    assert_eq!(results1[0].name, "Host 1 Listing");

    // Query with exact email for User 2
    let filter2 = common::models::ListingFilter {
        name: None,
        country: None,
        min_price: None,
        max_price: None,
        structure_type: vec![],
        owner: Some(u2.email.clone()),
        resolution: None,
        currency: None,
        user_id: None,
        host: None,
        include_inactive: false,
        ..Default::default()
    };
    let results2 = get_listings(&mut *tx, 1, 10, Some(filter2)).await.unwrap();
    assert_eq!(results2.len(), 1);
    assert_eq!(results2[0].name, "Host 2 Listing");

    // Query strictly by user_id for User 1
    let filter_uid1 = common::models::ListingFilter {
        name: None,
        country: None,
        min_price: None,
        max_price: None,
        structure_type: vec![],
        owner: None,
        resolution: None,
        currency: None,
        user_id: Some(u1.id),
        host: None,
        include_inactive: false,
        ..Default::default()
    };
    let results_uid1 = get_listings(&mut *tx, 1, 10, Some(filter_uid1))
        .await
        .unwrap();
    assert_eq!(results_uid1.len(), 1);
    assert_eq!(results_uid1[0].name, "Host 1 Listing");

    // Query strictly by user_id for User 2
    let filter_uid2 = common::models::ListingFilter {
        name: None,
        country: None,
        min_price: None,
        max_price: None,
        structure_type: vec![],
        owner: None,
        resolution: None,
        currency: None,
        user_id: Some(u2.id),
        host: None,
        include_inactive: false,
        ..Default::default()
    };
    let results_uid2 = get_listings(&mut *tx, 1, 10, Some(filter_uid2))
        .await
        .unwrap();
    assert_eq!(results_uid2.len(), 1);
    assert_eq!(results_uid2[0].name, "Host 2 Listing");

    // Public host filter: by slug only, and each result carries it.
    let by_host = |host: &str| common::models::ListingFilter {
        host: Some(host.to_string()),
        ..Default::default()
    };
    let results_host1 = get_listings(&mut *tx, 1, 10, Some(by_host(&u1.slug)))
        .await
        .unwrap();
    assert_eq!(results_host1.len(), 1);
    assert_eq!(results_host1[0].name, "Host 1 Listing");
    assert_eq!(
        results_host1[0].owner_slug.as_deref(),
        Some(u1.slug.as_str())
    );
    let results_unknown = get_listings(&mut *tx, 1, 10, Some(by_host("no-such-host-xyz")))
        .await
        .unwrap();
    assert!(results_unknown.is_empty());
}

#[tokio::test]
async fn test_create_and_update_listing_with_fees() {
    let mut conn = setup_test_db().await;
    let mut tx = conn.begin().await.expect("Failed to begin transaction");

    let user_id = create_test_user_with_host_profile(&mut tx).await;
    let suffix = Uuid::now_v7();
    let new_listing = NewListing {
        name: format!("Fee Test Villa {}", suffix),
        user_id,
        description: Some("Villa with fees".to_string()),
        listing_structure_id: 5,
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(350.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        max_guests: 6,
        bedrooms: 3,
        beds: 3,
        full_bathrooms: 2,
        half_bathrooms: 1,
        square_meters: Some(250),
        latitude: None,
        longitude: None,
        listing_details: None,
        city: Some("Ocho Rios".to_string()),
        base_currency: "USD".to_string(),
        minimum_stay: 2,
        days_between_bookings: 1,
        commission_pct: Some(dec!(0.1000)),
        cleaning_fee: Some(dec!(75.00)),
        damage_deposit: Some(dec!(300.00)),
        guests_included: Some(2),
        extra_guest_fee: Some(dec!(25.00)),
        is_active: true,
    };

    let created = create_listing(&mut *tx, &new_listing).await.unwrap();
    assert_eq!(created.cleaning_fee, dec!(75.00));
    assert_eq!(created.damage_deposit, dec!(300.00));
    assert_eq!(created.guests_included, 2);
    assert_eq!(created.extra_guest_fee, dec!(25.00));

    // Defaulting test (None defaults to 0.00 / 1)
    let no_fees_listing = NewListing {
        name: format!("No Fees Villa {}", suffix),
        user_id,
        description: None,
        listing_structure_id: 1,
        country: "Jamaica".to_string(),
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
        commission_pct: Some(dec!(0.0000)),
        cleaning_fee: None,
        damage_deposit: None,
        guests_included: None,
        extra_guest_fee: None,
        is_active: true,
    };
    let created_no_fees = create_listing(&mut *tx, &no_fees_listing).await.unwrap();
    assert_eq!(created_no_fees.cleaning_fee, dec!(0.00));
    assert_eq!(created_no_fees.damage_deposit, dec!(0.00));
    assert_eq!(created_no_fees.guests_included, 1);
    assert_eq!(created_no_fees.extra_guest_fee, dec!(0.00));

    let update_data = UpdatedListing {
        cleaning_fee: Some(dec!(120.00)),
        damage_deposit: Some(dec!(500.00)),
        guests_included: Some(4),
        extra_guest_fee: Some(dec!(35.00)),
        ..Default::default()
    };
    let updated = update_listing_with_conn(&mut tx, created.id, &update_data)
        .await
        .unwrap();
    assert_eq!(updated.cleaning_fee, dec!(120.00));
    assert_eq!(updated.damage_deposit, dec!(500.00));
    assert_eq!(updated.guests_included, 4);
    assert_eq!(updated.extra_guest_fee, dec!(35.00));

    // Verify guests_included > max_guests fails
    let invalid_update = UpdatedListing {
        guests_included: Some(10),
        ..Default::default()
    };
    let invalid_res = update_listing_with_conn(&mut tx, created.id, &invalid_update).await;
    assert!(matches!(invalid_res, Err(DbError::ValidationError(_))));
}

mod search_filter_tests {
    use super::*;
    use chrono::NaiveDate;
    use common::models::ListingFilter;
    use sqlx::PgConnection;

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2030, m, d).unwrap()
    }

    async fn new_listing(conn: &mut PgConnection, host: Uuid, name: &str) -> Uuid {
        let listing = NewListing {
            name: format!("{name} {}", Uuid::now_v7()),
            user_id: host,
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
            commission_pct: None,
            cleaning_fee: None,
            damage_deposit: None,
            guests_included: None,
            extra_guest_fee: None,
            is_active: true,
        };
        create_listing(&mut *conn, &listing).await.unwrap().id
    }

    async fn set(conn: &mut PgConnection, listing: Uuid, assignment: &str) {
        sqlx::query(&format!("UPDATE listing SET {assignment} WHERE id = $1"))
            .bind(listing)
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    /// A booking with the given status. `expires` is SQL for its `expires_at`.
    async fn booking(
        conn: &mut PgConnection,
        guest: Uuid,
        listing: Uuid,
        from: NaiveDate,
        to: NaiveDate,
        status: &str,
        expires: &str,
    ) {
        sqlx::query(&format!(
            r#"
            INSERT INTO booking (
                id, confirmation_code, guest_id, listing_id, status,
                date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                sub_total_price, discount_value, tax_value, fee_breakdown,
                total_price, cancellation_policy, metadata,
                cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
                created_at, updated_at, expires_at
            )
            VALUES (
                $1, $2, $3, $4, '{status}'::booking_status,
                $5, $6, 'USD', 200.00, 2, ($6 - $5),
                400.00, 0.00, 60.00, '[]'::jsonb,
                460.00, 'flexible'::cancellation_policy, '{{"num_adults": 2}}'::jsonb,
                0.00, 0.00, 20.00, 0.1500, 1.000000,
                NOW(), NOW(), {expires}
            )
            "#
        ))
        .bind(Uuid::now_v7())
        .bind(format!("S{}", Uuid::new_v4().simple())[..10].to_uppercase())
        .bind(guest)
        .bind(listing)
        .bind(from)
        .bind(to)
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    fn stay(host: Uuid, from: NaiveDate, to: NaiveDate) -> ListingFilter {
        ListingFilter {
            user_id: Some(host),
            check_in: Some(from),
            check_out: Some(to),
            ..Default::default()
        }
    }

    async fn found(conn: &mut PgConnection, filter: ListingFilter) -> Vec<Uuid> {
        get_listings(&mut *conn, 1, 50, Some(filter))
            .await
            .unwrap()
            .into_iter()
            .map(|l| l.id)
            .collect()
    }

    #[tokio::test]
    async fn a_stay_leaves_out_listings_that_are_taken() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.unwrap();
        let host = create_test_user_with_host_profile(&mut tx).await;
        let guest = create_test_user_no_profile(&mut tx).await;
        let (from, to) = (day(8, 10), day(8, 15));

        let free = new_listing(&mut tx, host, "Free").await;
        let confirmed = new_listing(&mut tx, host, "Confirmed").await;
        booking(&mut tx, guest, confirmed, from, to, "confirmed", "NULL").await;
        let live = new_listing(&mut tx, host, "Live hold").await;
        booking(
            &mut tx,
            guest,
            live,
            from,
            to,
            "pending",
            "NOW() + INTERVAL '1 hour'",
        )
        .await;
        let expired = new_listing(&mut tx, host, "Expired hold").await;
        booking(
            &mut tx,
            guest,
            expired,
            from,
            to,
            "pending",
            "NOW() - INTERVAL '1 hour'",
        )
        .await;
        let cancelled = new_listing(&mut tx, host, "Cancelled").await;
        booking(&mut tx, guest, cancelled, from, to, "cancelled", "NULL").await;

        let results = found(&mut tx, stay(host, from, to)).await;
        assert!(results.contains(&free));
        assert!(results.contains(&expired));
        assert!(results.contains(&cancelled));
        assert!(!results.contains(&confirmed));
        assert!(!results.contains(&live));
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn the_gap_applies_and_same_day_turnover_stays_free_without_one() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.unwrap();
        let host = create_test_user_with_host_profile(&mut tx).await;
        let guest = create_test_user_no_profile(&mut tx).await;
        let villa = new_listing(&mut tx, host, "Gap").await;
        booking(
            &mut tx,
            guest,
            villa,
            day(8, 10),
            day(8, 15),
            "confirmed",
            "NULL",
        )
        .await;

        // No gap: same-day turnover either way.
        assert_eq!(
            found(&mut tx, stay(host, day(8, 15), day(8, 20))).await,
            [villa]
        );
        assert_eq!(
            found(&mut tx, stay(host, day(8, 5), day(8, 10))).await,
            [villa]
        );
        assert!(
            found(&mut tx, stay(host, day(8, 14), day(8, 20)))
                .await
                .is_empty()
        );

        set(&mut tx, villa, "days_between_bookings = 2").await;
        assert!(
            found(&mut tx, stay(host, day(8, 16), day(8, 20)))
                .await
                .is_empty()
        );
        assert_eq!(
            found(&mut tx, stay(host, day(8, 17), day(8, 20))).await,
            [villa]
        );
        assert!(
            found(&mut tx, stay(host, day(8, 5), day(8, 9)))
                .await
                .is_empty()
        );
        assert_eq!(
            found(&mut tx, stay(host, day(8, 5), day(8, 8))).await,
            [villa]
        );
    }

    #[tokio::test]
    async fn a_stored_gap_outside_the_limits_cant_break_the_search() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.unwrap();
        let host = create_test_user_with_host_profile(&mut tx).await;
        let guest = create_test_user_no_profile(&mut tx).await;
        let huge = new_listing(&mut tx, host, "Huge gap").await;
        booking(
            &mut tx,
            guest,
            huge,
            day(8, 10),
            day(8, 15),
            "confirmed",
            "NULL",
        )
        .await;
        set(&mut tx, huge, "days_between_bookings = 2147483647").await;
        let other = new_listing(&mut tx, host, "Other").await;

        // Well inside the 365-day limit of the huge gap, but far from overflowing.
        assert_eq!(
            found(&mut tx, stay(host, day(10, 1), day(10, 5))).await,
            [other]
        );

        let negative = new_listing(&mut tx, host, "Negative gap").await;
        booking(
            &mut tx,
            guest,
            negative,
            day(10, 1),
            day(10, 5),
            "confirmed",
            "NULL",
        )
        .await;
        set(&mut tx, negative, "days_between_bookings = -5").await;
        let results = found(&mut tx, stay(host, day(10, 4), day(10, 6))).await;
        assert!(!results.contains(&negative), "the rule doesn't shrink");
    }

    #[tokio::test]
    async fn guests_must_fit_the_listing() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.unwrap();
        let host = create_test_user_with_host_profile(&mut tx).await;
        let villa = new_listing(&mut tx, host, "Four guests").await;
        let filter = |guests| ListingFilter {
            user_id: Some(host),
            guests: Some(guests),
            ..Default::default()
        };

        assert_eq!(found(&mut tx, filter(4)).await, [villa]);
        assert!(found(&mut tx, filter(5)).await.is_empty());
    }

    #[tokio::test]
    async fn the_stay_must_meet_the_minimum_including_overrides() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.unwrap();
        let host = create_test_user_with_host_profile(&mut tx).await;
        let villa = new_listing(&mut tx, host, "Minimum").await;
        set(&mut tx, villa, "minimum_stay = 3").await;

        assert!(
            found(&mut tx, stay(host, day(8, 1), day(8, 3)))
                .await
                .is_empty()
        );
        assert_eq!(
            found(&mut tx, stay(host, day(8, 1), day(8, 4))).await,
            [villa]
        );

        // An override covering only part of the stay still sets the minimum.
        sqlx::query(
            "INSERT INTO listing_price_overrides (listing_id, start_date, end_date, nightly_rate, min_nights)
             VALUES ($1, $2, $3, 300.00, 5)",
        )
        .bind(villa)
        .bind(day(8, 3))
        .bind(day(8, 10))
        .execute(&mut *tx)
        .await
        .unwrap();
        assert!(
            found(&mut tx, stay(host, day(8, 1), day(8, 5)))
                .await
                .is_empty()
        );
        assert_eq!(
            found(&mut tx, stay(host, day(8, 1), day(8, 6))).await,
            [villa]
        );
        // A stay that ends the day the override starts doesn't touch it.
        assert_eq!(
            found(&mut tx, stay(host, day(7, 30), day(8, 3))).await,
            [villa]
        );
    }

    #[tokio::test]
    async fn dated_searches_leave_out_listings_that_cant_be_booked() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.unwrap();
        let host = create_test_user_with_host_profile(&mut tx).await;
        let bookable = new_listing(&mut tx, host, "Bookable").await;
        let inactive = new_listing(&mut tx, host, "Inactive").await;
        set(&mut tx, inactive, "is_active = FALSE").await;
        let unpriced = new_listing(&mut tx, host, "Unpriced").await;
        set(&mut tx, unpriced, "price_per_night = NULL").await;

        let mut dated = stay(host, day(8, 1), day(8, 4));
        dated.include_inactive = true;
        assert_eq!(found(&mut tx, dated).await, [bookable]);

        // Without dates, both still show as they do today.
        let undated = ListingFilter {
            user_id: Some(host),
            include_inactive: true,
            ..Default::default()
        };
        assert_eq!(found(&mut tx, undated).await.len(), 3);
    }

    #[tokio::test]
    async fn pages_are_full_when_earlier_listings_were_filtered_out() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.unwrap();
        let host = create_test_user_with_host_profile(&mut tx).await;
        let guest = create_test_user_no_profile(&mut tx).await;
        let (from, to) = (day(8, 10), day(8, 15));
        let mut free = Vec::new();
        for n in 0..3 {
            free.push(new_listing(&mut tx, host, &format!("Free {n}")).await);
        }
        // Results are newest first, so these would fill the first page.
        for n in 0..3 {
            let taken = new_listing(&mut tx, host, &format!("Taken {n}")).await;
            booking(&mut tx, guest, taken, from, to, "confirmed", "NULL").await;
        }

        let first = get_listings(&mut *tx, 1, 2, Some(stay(host, from, to)))
            .await
            .unwrap();
        assert_eq!(first.len(), 2);
        assert!(first.iter().all(|l| free.contains(&l.id)));
    }
}
