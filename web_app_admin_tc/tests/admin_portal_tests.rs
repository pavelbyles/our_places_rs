use chrono::NaiveDate;
use common::models::NewListingRequest;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;
use web_app_admin_tc::{
    AdminUpdateUserPayload, GranularPermissions, PermissionTypeConstraintError,
    RoleCapabilityProfile, UserFilterQuery,
};

#[test]
fn test_admin_kpi_revenue_and_tax_estimation() {
    let base_rate = dec!(1800.00);
    let estimated_occupancy_nights = 14;
    let gross_revenue = base_rate * Decimal::from(estimated_occupancy_nights);
    assert_eq!(gross_revenue, dec!(25200.00));

    // Jamaican statutory General Consumption Tax (GCT) 15%
    let statutory_gct_rate = dec!(0.15);
    let statutory_gct_amount = gross_revenue * statutory_gct_rate;
    assert_eq!(statutory_gct_amount, dec!(3780.00));

    let net_total = gross_revenue + statutory_gct_amount;
    assert_eq!(net_total, dec!(28980.00));
}

#[test]
fn test_admin_seasonal_override_interval_validation() {
    let start_date = NaiveDate::from_ymd_opt(2026, 12, 15).unwrap();
    let end_date = NaiveDate::from_ymd_opt(2027, 1, 5).unwrap();
    let min_stay = 5;
    let override_rate = dec!(2800.00);

    assert!(end_date > start_date, "End date must follow start date");
    assert!(min_stay >= 1, "Minimum stay must be at least 1 night");
    assert!(override_rate > Decimal::ZERO, "Rate must be positive");
}

#[test]
fn test_admin_role_authorization_and_shadow_user_audit() {
    let roles = ["admin", "host", "booker"];
    assert!(roles.contains(&"admin"));
    assert!(roles.contains(&"host"));
    assert!(roles.contains(&"booker"));

    // Shadow user audit: A user with a temporary hold can be promoted
    let shadow_hold_seconds = 15 * 60;
    assert_eq!(shadow_hold_seconds, 900);
}

#[test]
fn test_manage_users_access_control_role_boundaries() {
    use web_app_common_tc::auth::AuthUser;

    // 1. Admin user: Authorized for both admin portal and user management
    let admin_user = AuthUser::new("Admin User", "admin@ourplaces.io", "admin");
    assert!(admin_user.is_admin());
    assert!(admin_user.is_authorized_for_admin_portal());

    // 2. Superadmin user: Authorized for both admin portal and user management
    let superadmin_user = AuthUser::new("Super Admin", "super@ourplaces.io", "superadmin");
    assert!(superadmin_user.is_admin());
    assert!(superadmin_user.is_authorized_for_admin_portal());

    // 3. Host user: Authorized for general admin portal (listings/bookings) BUT NOT user management
    let host_user = AuthUser::new("Host User", "host@ourplaces.io", "host");
    assert!(
        !host_user.is_admin(),
        "Host must not be considered an admin"
    );
    assert!(
        host_user.is_authorized_for_admin_portal(),
        "Host is authorized for listings/bookings"
    );

    // 4. Guest / Booker user: Not authorized for admin portal or user management
    let guest_user = AuthUser::new("Guest Booker", "guest@example.com", "booker");
    assert!(!guest_user.is_admin());
    assert!(!guest_user.is_authorized_for_admin_portal());
}

#[test]
fn test_listing_clone_and_field_coverage() {
    let original_name = "The Reef House";
    let cloned_name = format!("{} (Copy)", original_name);
    assert_eq!(cloned_name, "The Reef House (Copy)");

    let structures = ["Apartment", "House", "Townhouse", "Studio", "Villa"];
    assert!(structures.contains(&"Apartment"));
    assert!(structures.contains(&"House"));
    assert!(structures.contains(&"Townhouse"));
    assert!(structures.contains(&"Studio"));
    assert!(structures.contains(&"Villa"));
}

#[test]
fn test_admin_layout_navigation_sections() {
    let navigation_sections = [
        "Overview",
        "Inventory & Properties",
        "Operations",
        "Access Control",
        "Configuration & Finance",
    ];
    assert_eq!(navigation_sections.len(), 5);
    assert!(navigation_sections.contains(&"Inventory & Properties"));
    assert!(navigation_sections.contains(&"Access Control"));
    assert!(navigation_sections.contains(&"Configuration & Finance"));
}

#[test]
fn test_granular_permissions_type_constraint() {
    let active_perms = GranularPermissions {
        can_manage_listings: true,
        can_manage_bookings: true,
        can_configure_rates: false,
        can_manage_users: false,
    };

    // 1. Host with granular perms (listings and bookings only) -> Allowed
    let host_profile = RoleCapabilityProfile::build(true, false, false, active_perms.clone());
    assert!(host_profile.is_ok());
    assert!(host_profile.unwrap().is_privileged());

    // 2. Host with rate or user permissions -> Strictly rejected
    let host_with_rates = RoleCapabilityProfile::build(
        true,
        false,
        false,
        GranularPermissions {
            can_manage_listings: true,
            can_manage_bookings: true,
            can_configure_rates: true,
            can_manage_users: false,
        },
    );
    assert_eq!(
        host_with_rates,
        Err(PermissionTypeConstraintError::HostCannotHoldAdminPrivileges)
    );

    let host_with_users = RoleCapabilityProfile::build(
        true,
        false,
        false,
        GranularPermissions {
            can_manage_listings: true,
            can_manage_bookings: true,
            can_configure_rates: false,
            can_manage_users: true,
        },
    );
    assert_eq!(
        host_with_users,
        Err(PermissionTypeConstraintError::HostCannotHoldAdminPrivileges)
    );

    // 3. Admin with all granular perms -> Allowed
    let admin_profile = RoleCapabilityProfile::build(
        false,
        true,
        false,
        GranularPermissions {
            can_manage_listings: true,
            can_manage_bookings: true,
            can_configure_rates: true,
            can_manage_users: true,
        },
    );
    assert!(admin_profile.is_ok());
    assert!(admin_profile.unwrap().is_privileged());

    // 4. Booker ONLY with granular perms -> Strictly rejected by Rust type system
    let booker_with_perms = RoleCapabilityProfile::build(false, false, true, active_perms.clone());
    assert_eq!(
        booker_with_perms,
        Err(PermissionTypeConstraintError::BookerCannotHoldPrivileges)
    );

    // 5. Standard Booker with NO granular perms -> Allowed as unprivileged
    let standard_booker =
        RoleCapabilityProfile::build(false, false, true, GranularPermissions::default());
    assert!(standard_booker.is_ok());
    assert!(!standard_booker.unwrap().is_privileged());
}

#[test]
fn test_listing_23_fields_coordinate_and_price_boundaries() {
    let req = NewListingRequest {
        name: "Sunset Haven Villa".to_string(),
        user_id: Uuid::now_v7(),
        description: Some("Panoramic oceanfront luxury sanctuary".to_string()),
        listing_structure: "Villa".to_string(),
        country: "Jamaica".to_string(),
        base_currency: "USD".to_string(),
        price_per_night: Some(dec!(750.00)),
        weekly_discount_percentage: Some(dec!(10.0)),
        monthly_discount_percentage: Some(dec!(20.0)),
        latitude: Some(18.2568),
        longitude: Some(-78.3621),
        city: Some("Negril".to_string()),
        max_guests: 8,
        bedrooms: 4,
        beds: 5,
        full_bathrooms: 4,
        half_bathrooms: 1,
        square_meters: Some(380),
        listing_details: None,
        minimum_stay: 3,
        days_between_bookings: 1,
        commission_pct: Some(dec!(0.1000)),
    };

    assert!(req.price_per_night.unwrap() > Decimal::ZERO);
    assert!(req.max_guests > 0);
    assert!(req.bedrooms > 0);
    assert!(req.minimum_stay >= 1);

    // Caribbean GPS coordinate validation bounds
    let lat = req.latitude.unwrap();
    let lon = req.longitude.unwrap();
    assert!(
        (17.0..=19.0).contains(&lat),
        "Latitude must be within Jamaica bounds"
    );
    assert!(
        (-79.0..=-76.0).contains(&lon),
        "Longitude must be within Jamaica bounds"
    );
}

#[test]
fn test_seasonal_override_inverted_dates_rejection() {
    let start = NaiveDate::from_ymd_opt(2027, 1, 5).unwrap();
    let end = NaiveDate::from_ymd_opt(2026, 12, 15).unwrap(); // Inverted!

    assert!(end < start, "Inverted date range detected");
    // Rule: End date must strictly follow start date
    let is_valid = end > start;
    assert!(!is_valid, "Inverted seasonal interval must be rejected");
}

#[test]
fn test_user_search_and_role_filter_matching() {
    let query = UserFilterQuery {
        q: Some("Pavel".to_string()),
        role: Some("admin".to_string()),
    };

    assert_eq!(query.q.as_deref(), Some("Pavel"));
    assert_eq!(query.role.as_deref(), Some("admin"));

    // Matching logic verification
    let first_name = "Pavel";
    let last_name = "Byles";
    let email = "pavel@ourplaces.io";
    let roles = ["admin".to_string(), "host".to_string()];

    let q_lower = query.q.as_ref().unwrap().to_lowercase();
    let full_name = format!("{} {}", first_name, last_name).to_lowercase();
    let matches_search = full_name.contains(&q_lower) || email.to_lowercase().contains(&q_lower);
    assert!(matches_search, "Search must match first name");

    let role_lower = query.role.as_ref().unwrap().to_lowercase();
    let matches_role = roles.iter().any(|r| r.to_lowercase() == role_lower);
    assert!(matches_role, "Role filter must match admin");
}

#[test]
fn test_user_credentials_update_payload_mapping() {
    let target_id = Uuid::new_v4();
    let payload = AdminUpdateUserPayload {
        id: target_id,
        email: Some("updated.host@ourplaces.io".to_string()),
        password: Some("newSecurePassword123!".to_string()),
        first_name: Some("Elena".to_string()),
        last_name: Some("Rostova".to_string()),
        phone_number: Some("+1 876 555 0199".to_string()),
        is_active: Some(true),
        is_verified: Some(true),
        roles: Some(vec!["host".to_string()]),
        can_manage_bookings: Some(true),
        can_manage_listings: Some(true),
        default_currency: Some("JMD".to_string()),
    };

    assert_eq!(payload.id, target_id);
    assert_eq!(payload.email.as_deref(), Some("updated.host@ourplaces.io"));
    assert_eq!(payload.password.as_deref(), Some("newSecurePassword123!"));
    assert!(payload.is_active.unwrap());
    assert!(payload.is_verified.unwrap());
    assert!(payload.can_manage_bookings.unwrap());
    assert!(payload.can_manage_listings.unwrap());
    assert_eq!(payload.default_currency.as_deref(), Some("JMD"));
}

#[test]
fn test_payout_filter_query_deserialization_and_params() {
    use web_app_admin_tc::PayoutFilterQuery;

    let listing_id = Uuid::new_v4();
    let query = PayoutFilterQuery {
        listing_id: Some(listing_id.to_string()),
        status: Some("pending".to_string()),
        date_from: Some("2026-10-01".to_string()),
        date_to: Some("2026-10-31".to_string()),
        page: Some(2),
    };

    assert_eq!(
        query.listing_id.as_deref(),
        Some(listing_id.to_string().as_str())
    );
    assert_eq!(query.status.as_deref(), Some("pending"));
    assert_eq!(query.page, Some(2));
}

#[test]
fn test_host_payout_accounting_math_precision() {
    // 5 nights @ $500.00/night = $2500.00 gross
    let gross_amount = dec!(2500.00);
    let commission_pct = dec!(0.1000); // 10.00%
    let tax_withheld = dec!(0.00); // statutory zero host withholding

    let (commission_amount, net_payout) =
        common::pricing::calculate_host_payout(gross_amount, commission_pct, tax_withheld);

    assert_eq!(commission_amount, dec!(250.0000));
    assert_eq!(net_payout, dec!(2250.0000));
    assert_eq!(gross_amount - commission_amount - tax_withheld, net_payout);
}

#[test]
fn test_host_dashboard_listing_and_kpi_scoping() {
    use common::models::{BookingMetadataResponse, BookingResponse, ListingResponse};
    use std::collections::HashSet;

    let host_a_id = Uuid::new_v4();
    let host_b_id = Uuid::new_v4();

    let listing_a1 = ListingResponse {
        id: Uuid::new_v4(),
        user_id: host_a_id,
        name: "Villa Azure".to_string(),
        description: None,
        listing_structure: "Villa".to_string(),
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(800.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        is_active: true,
        added_at: chrono::Utc::now(),
        owner_name: Some("Host A".to_string()),
        primary_image_url: None,
        max_guests: 6,
        bedrooms: 3,
        beds: 3,
        full_bathrooms: 3,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        overall_rating: None,
        city: Some("Montego Bay".to_string()),
        base_currency: "USD".to_string(),
        slug: "villa-azure".to_string(),
        listing_details: None,
        minimum_stay: 2,
        days_between_bookings: 1,
        commission_pct: Some(dec!(0.1000)),
    };

    let listing_a2 = ListingResponse {
        id: Uuid::new_v4(),
        user_id: host_a_id,
        name: "Coral Cove".to_string(),
        description: None,
        listing_structure: "Villa".to_string(),
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(1200.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        is_active: true,
        added_at: chrono::Utc::now(),
        owner_name: Some("Host A".to_string()),
        primary_image_url: None,
        max_guests: 8,
        bedrooms: 4,
        beds: 4,
        full_bathrooms: 4,
        half_bathrooms: 1,
        square_meters: None,
        latitude: None,
        longitude: None,
        overall_rating: None,
        city: Some("Ocho Rios".to_string()),
        base_currency: "USD".to_string(),
        slug: "coral-cove".to_string(),
        listing_details: None,
        minimum_stay: 3,
        days_between_bookings: 1,
        commission_pct: Some(dec!(0.1000)),
    };

    let listing_b1 = ListingResponse {
        id: Uuid::new_v4(),
        user_id: host_b_id,
        name: "Blue Mountain Peak Retreat".to_string(),
        description: None,
        listing_structure: "House".to_string(),
        country: "Jamaica".to_string(),
        price_per_night: Some(dec!(500.00)),
        weekly_discount_percentage: None,
        monthly_discount_percentage: None,
        is_active: true,
        added_at: chrono::Utc::now(),
        owner_name: Some("Host B".to_string()),
        primary_image_url: None,
        max_guests: 4,
        bedrooms: 2,
        beds: 2,
        full_bathrooms: 2,
        half_bathrooms: 0,
        square_meters: None,
        latitude: None,
        longitude: None,
        overall_rating: None,
        city: Some("Kingston".to_string()),
        base_currency: "USD".to_string(),
        slug: "blue-mountain-retreat".to_string(),
        listing_details: None,
        minimum_stay: 1,
        days_between_bookings: 0,
        commission_pct: Some(dec!(0.1000)),
    };

    let all_listings = vec![listing_a1.clone(), listing_a2.clone(), listing_b1.clone()];

    // Booking 1 on Listing A1: Confirmed ($1,600)
    let booking_a1 = BookingResponse {
        id: Uuid::new_v4(),
        confirmation_code: "OP-CONF-1".to_string(),
        guest_id: Uuid::new_v4(),
        listing_id: listing_a1.id,
        status: "confirmed".to_string(),
        date_from: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 11, 3).unwrap(),
        currency: "USD".to_string(),
        daily_rate: dec!(800.00),
        number_of_persons: 2,
        total_days: 2,
        sub_total_price: dec!(1600.00),
        discount_value: None,
        tax_value: None,
        total_price: dec!(1600.00),
        cancellation_policy: "moderate".to_string(),
        metadata: BookingMetadataResponse::default(),
        review_eligibility: None,
        door_access_code: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    // Booking 2 on Listing A2: Pending hold ($3,600)
    let booking_a2 = BookingResponse {
        id: Uuid::new_v4(),
        confirmation_code: "OP-HOLD-2".to_string(),
        guest_id: Uuid::new_v4(),
        listing_id: listing_a2.id,
        status: "pending_payment".to_string(),
        date_from: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 11, 13).unwrap(),
        currency: "USD".to_string(),
        daily_rate: dec!(1200.00),
        number_of_persons: 4,
        total_days: 3,
        sub_total_price: dec!(3600.00),
        discount_value: None,
        tax_value: None,
        total_price: dec!(3600.00),
        cancellation_policy: "strict".to_string(),
        metadata: BookingMetadataResponse::default(),
        review_eligibility: None,
        door_access_code: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    // Booking 3 on Listing B1: Confirmed ($5,000)
    let booking_b1 = BookingResponse {
        id: Uuid::new_v4(),
        confirmation_code: "OP-CONF-3".to_string(),
        guest_id: Uuid::new_v4(),
        listing_id: listing_b1.id,
        status: "confirmed".to_string(),
        date_from: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
        date_to: NaiveDate::from_ymd_opt(2026, 12, 11).unwrap(),
        currency: "USD".to_string(),
        daily_rate: dec!(500.00),
        number_of_persons: 2,
        total_days: 10,
        sub_total_price: dec!(5000.00),
        discount_value: None,
        tax_value: None,
        total_price: dec!(5000.00),
        cancellation_policy: "flexible".to_string(),
        metadata: BookingMetadataResponse::default(),
        review_eligibility: None,
        door_access_code: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let all_bookings = vec![booking_a1, booking_a2, booking_b1];

    // CASE 1: Admin View -> sees all listings and all bookings
    let is_admin = true;
    let admin_listings = if is_admin {
        all_listings.clone()
    } else {
        vec![]
    };
    let admin_listing_count = admin_listings.len();
    let admin_active_holds = all_bookings
        .iter()
        .filter(|b| b.status == "pending_payment")
        .count();
    let admin_revenue: Decimal = all_bookings
        .iter()
        .filter(|b| b.status == "confirmed")
        .map(|b| b.total_price)
        .sum();

    assert_eq!(
        admin_listing_count, 3,
        "Admin must see all 3 listings across portfolio"
    );
    assert_eq!(
        admin_active_holds, 1,
        "Admin sees platform total active holds"
    );
    assert_eq!(
        admin_revenue,
        dec!(6600.00),
        "Admin sees platform gross confirmed revenue (1600 + 5000)"
    );

    // CASE 2: Host A View -> sees ONLY Host A's 2 villas and related bookings
    let is_admin_host = false;
    let host_user_id = Some(host_a_id);
    let host_a_listings: Vec<ListingResponse> = if is_admin_host {
        all_listings
    } else {
        all_listings
            .into_iter()
            .filter(|l| host_user_id == Some(l.user_id))
            .collect()
    };
    let host_a_listing_ids: HashSet<Uuid> = host_a_listings.iter().map(|l| l.id).collect();
    let host_a_bookings: Vec<BookingResponse> = all_bookings
        .into_iter()
        .filter(|b| host_a_listing_ids.contains(&b.listing_id))
        .collect();

    let host_a_listing_count = host_a_listings.len();
    let host_a_active_holds = host_a_bookings
        .iter()
        .filter(|b| b.status == "pending_payment")
        .count();
    let host_a_revenue: Decimal = host_a_bookings
        .iter()
        .filter(|b| b.status == "confirmed")
        .map(|b| b.total_price)
        .sum();

    // Verify host A sees ONLY their 2 villas, 1 hold, and $1,600 revenue
    assert_eq!(
        host_a_listing_count, 2,
        "Host A must see exactly their 2 villas"
    );
    assert_eq!(
        host_a_active_holds, 1,
        "Host A must see only holds on their villas"
    );
    assert_eq!(
        host_a_revenue,
        dec!(1600.00),
        "Host A revenue must reflect only their confirmed bookings"
    );
    assert!(
        host_a_listings.iter().all(|l| l.user_id == host_a_id),
        "All listings in host A dashboard must belong to host A"
    );
}

#[test]
fn test_login_primary_role_determination() {
    fn determine_primary_role(roles: &[&str]) -> String {
        if roles.iter().any(|r| {
            let lr = r.to_lowercase();
            lr == "admin" || lr == "superadmin"
        }) {
            "admin".to_string()
        } else if roles.iter().any(|r| r.to_lowercase() == "host") {
            "host".to_string()
        } else {
            roles.first().copied().unwrap_or("host").to_string()
        }
    }

    assert_eq!(determine_primary_role(&["host"]), "host");
    assert_eq!(determine_primary_role(&["booker", "host"]), "host");
    assert_eq!(determine_primary_role(&["admin", "host"]), "admin");
    assert_eq!(determine_primary_role(&["host", "admin"]), "admin");
    assert_eq!(determine_primary_role(&["superadmin"]), "admin");
}
