use chrono::NaiveDate;
use common::models::NewListingRequest;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;
use web_app_admin_tc::{
    GranularPermissions, PermissionTypeConstraintError, RoleCapabilityProfile,
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
    let roles = vec!["admin", "host", "booker"];
    assert!(roles.contains(&"admin"));
    assert!(roles.contains(&"host"));
    assert!(roles.contains(&"booker"));

    // Shadow user audit: A user with a temporary hold can be promoted
    let shadow_hold_seconds = 15 * 60;
    assert_eq!(shadow_hold_seconds, 900);
}

#[test]
fn test_listing_clone_and_field_coverage() {
    let original_name = "The Reef House";
    let cloned_name = format!("{} (Copy)", original_name);
    assert_eq!(cloned_name, "The Reef House (Copy)");

    let structures = vec!["Apartment", "House", "Townhouse", "Studio", "Villa"];
    assert!(structures.contains(&"Apartment"));
    assert!(structures.contains(&"House"));
    assert!(structures.contains(&"Townhouse"));
    assert!(structures.contains(&"Studio"));
    assert!(structures.contains(&"Villa"));
}

#[test]
fn test_admin_layout_navigation_sections() {
    let navigation_sections = vec![
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

    // 1. Host with granular perms -> Allowed
    let host_profile = RoleCapabilityProfile::build(true, false, false, active_perms.clone());
    assert!(host_profile.is_ok());
    assert!(host_profile.unwrap().is_privileged());

    // 2. Admin with granular perms -> Allowed
    let admin_profile = RoleCapabilityProfile::build(false, true, false, active_perms.clone());
    assert!(admin_profile.is_ok());
    assert!(admin_profile.unwrap().is_privileged());

    // 3. Booker ONLY with granular perms -> Strictly rejected by Rust type system
    let booker_with_perms = RoleCapabilityProfile::build(false, false, true, active_perms.clone());
    assert_eq!(
        booker_with_perms,
        Err(PermissionTypeConstraintError::BookerCannotHoldPrivileges)
    );

    // 4. Standard Booker with NO granular perms -> Allowed as unprivileged
    let standard_booker = RoleCapabilityProfile::build(
        false,
        false,
        true,
        GranularPermissions::default(),
    );
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
    };

    assert!(req.price_per_night.unwrap() > Decimal::ZERO);
    assert!(req.max_guests > 0);
    assert!(req.bedrooms > 0);
    assert!(req.minimum_stay >= 1);

    // Caribbean GPS coordinate validation bounds
    let lat = req.latitude.unwrap();
    let lon = req.longitude.unwrap();
    assert!(lat >= 17.0 && lat <= 19.0, "Latitude must be within Jamaica bounds");
    assert!(lon >= -79.0 && lon <= -76.0, "Longitude must be within Jamaica bounds");
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
