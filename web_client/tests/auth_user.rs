//! Tests for the authenticated-user model moved from `web_app_common_tc`.

use web_client::auth::AuthUser;

#[test]
fn initials_table() {
    for (name, expected) in [("David Sterling", "DS"), ("Elena", "EL"), ("   ", "U")] {
        assert_eq!(
            AuthUser::new(name, "u@x.io", "guest").initials(),
            expected,
            "{name:?}"
        );
    }
}

#[test]
fn roles_map_to_portal_access() {
    for (role, ns, admin_portal, is_admin, display) in [
        ("host", "admin", true, false, "Host"),
        ("SuperAdmin", "admin", true, true, "Administrator"),
        ("guest", "guest", false, false, "Verified Guest"),
    ] {
        let user = AuthUser::with_namespace("U", "u@x.io", role, ns);
        assert_eq!(
            user.is_authorized_for_admin_portal(),
            admin_portal,
            "{role}"
        );
        assert_eq!(user.is_admin(), is_admin, "{role}");
        assert_eq!(user.role_display(), display, "{role}");
        assert_eq!(user.namespace, ns, "{role}");
    }
}
