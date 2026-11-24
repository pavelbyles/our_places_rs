//! Tests for typed sessions and route-guard decisions.

use chrono::Utc;
use common::models::SessionResponse;
use uuid::Uuid;
use web_client::session::{AdminRole, AdminSession, GuardError, GuestSession, login_redirect};

fn session(namespace: &str, role: &str, user_id: Uuid) -> SessionResponse {
    SessionResponse {
        token_hash: "ab".repeat(32),
        user_id,
        email: "g@x.io".into(),
        name: "Grace Hopper".into(),
        role: role.into(),
        namespace: namespace.into(),
        created_at: Utc::now(),
        last_accessed_at: Utc::now(),
        expires_at: 0,
    }
}

#[tokio::test]
async fn namespace_and_role_guard_matrix() {
    let id = Uuid::now_v7();

    // Guest session namespace + nil ID checks
    for (ns, uid, expected) in [
        ("guest", id, Ok(id)),
        ("admin", id, Err(GuardError::Unauthenticated)),
        ("guest", Uuid::nil(), Err(GuardError::Unauthenticated)),
    ] {
        assert_eq!(
            GuestSession::from_session(&session(ns, "guest", uid)).map(|s| s.user_id),
            expected,
            "guest ({ns}, {uid})"
        );
    }

    // Admin session namespace + role + guard escalation matrix
    for (ns, role, uid, expected_role, allows_host_or_admin, allows_admin_only) in [
        ("admin", "SuperAdmin", id, Ok(AdminRole::Admin), true, true),
        ("admin", "admin", id, Ok(AdminRole::Admin), true, true),
        ("admin", "host", id, Ok(AdminRole::Host), true, false),
        ("admin", "staff", id, Ok(AdminRole::Staff), false, false),
        (
            "admin",
            "guest",
            id,
            Err(GuardError::Forbidden),
            false,
            false,
        ),
        (
            "admin",
            "admin",
            Uuid::nil(),
            Err(GuardError::Unauthenticated),
            false,
            false,
        ),
    ] {
        let res = AdminSession::from_session(&session(ns, role, uid));
        assert_eq!(
            res.as_ref().map(|s| s.role),
            expected_role.as_ref().copied(),
            "admin ({ns}, {role}, {uid})"
        );
        if let Ok(s) = res {
            assert_eq!(
                s.clone().require_host_or_admin().is_ok(),
                allows_host_or_admin,
                "require_host_or_admin ({role})"
            );
            assert_eq!(
                s.require_admin_role().is_ok(),
                allows_admin_only,
                "require_admin_role ({role})"
            );
        }
    }

    assert_eq!(
        login_redirect("/checkout/villa-a?check_in=2027-01-02&adults=2"),
        "/login?redirect=%2Fcheckout%2Fvilla-a%3Fcheck_in%3D2027-01-02%26adults%3D2"
    );

    let cx = topcoat::context::Cx::default();
    assert_eq!(
        web_client::session::require_guest(&cx).await.err(),
        Some(GuardError::Unauthenticated)
    );
    assert_eq!(
        web_client::session::require_admin(&cx).await.err(),
        Some(GuardError::Unauthenticated)
    );
}
