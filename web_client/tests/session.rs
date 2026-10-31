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

#[test]
fn guest_session_from_guest_namespace() {
    let id = Uuid::now_v7();
    let s = GuestSession::from_session(&session("guest", "guest", id));
    assert_eq!(s.map(|s| s.user_id), Ok(id));
}

#[test]
fn guest_session_rejects_admin_namespace() {
    let s = GuestSession::from_session(&session("admin", "admin", Uuid::now_v7()));
    assert_eq!(s, Err(GuardError::Unauthenticated));
}

#[test]
fn nil_user_id_is_never_a_session() {
    assert_eq!(
        GuestSession::from_session(&session("guest", "guest", Uuid::nil())),
        Err(GuardError::Unauthenticated)
    );
    assert_eq!(
        AdminSession::from_session(&session("admin", "admin", Uuid::nil())),
        Err(GuardError::Unauthenticated)
    );
}

#[test]
fn admin_session_maps_roles() {
    let admin = AdminSession::from_session(&session("admin", "SuperAdmin", Uuid::now_v7()));
    let host = AdminSession::from_session(&session("admin", "host", Uuid::now_v7()));
    assert_eq!(admin.map(|s| s.role), Ok(AdminRole::Admin));
    assert_eq!(host.map(|s| s.role), Ok(AdminRole::Host));
}

#[test]
fn admin_session_forbids_guest_role() {
    let s = AdminSession::from_session(&session("admin", "guest", Uuid::now_v7()));
    assert_eq!(s, Err(GuardError::Forbidden));
}

#[test]
fn admin_role_guard_forbids_hosts() {
    let host = AdminSession::from_session(&session("admin", "host", Uuid::now_v7()));
    assert_eq!(
        host.and_then(AdminSession::require_admin_role),
        Err(GuardError::Forbidden)
    );
}

#[test]
fn login_redirect_encodes_next_path() {
    assert_eq!(
        login_redirect("/checkout/villa-a?check_in=2027-01-02&adults=2"),
        "/login?redirect=%2Fcheckout%2Fvilla-a%3Fcheck_in%3D2027-01-02%26adults%3D2"
    );
}
