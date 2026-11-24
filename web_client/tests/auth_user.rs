//! Tests for the authenticated-user model moved from `web_app_common_tc`.

use web_client::auth::AuthUser;

#[test]
fn initials_use_first_and_last_name() {
    assert_eq!(
        AuthUser::new("David Sterling", "d@x.io", "guest").initials(),
        "DS"
    );
}

#[test]
fn initials_use_first_two_letters_of_single_name() {
    assert_eq!(AuthUser::new("Elena", "e@x.io", "guest").initials(), "EL");
}

#[test]
fn initials_fall_back_to_u_for_blank_name() {
    assert_eq!(AuthUser::new("   ", "x@x.io", "guest").initials(), "U");
}

#[test]
fn roles_map_to_portal_access() {
    let host = AuthUser::with_namespace("H", "h@x.io", "host", "admin");
    let admin = AuthUser::with_namespace("A", "a@x.io", "SuperAdmin", "admin");
    let guest = AuthUser::new("G", "g@x.io", "guest");
    assert!(host.is_authorized_for_admin_portal() && !host.is_admin());
    assert!(admin.is_admin() && admin.role_display() == "Administrator");
    assert!(!guest.is_authorized_for_admin_portal());
    assert_eq!(guest.namespace, "guest");
}
