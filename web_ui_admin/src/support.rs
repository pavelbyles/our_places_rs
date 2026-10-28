use std::collections::HashSet;

use common::models::{BookingResponse, CreateSessionRequest, ListingResponse, UserResponse};
use topcoat::context::{Cx, try_app_context};
use uuid::Uuid;
use web_client::{
    auth::token_hash_to_hex,
    client::ListingSearchParams,
    get_api_client,
    jwt::ServiceJwt,
    session::{ADMIN_NAMESPACE, AdminRole, AdminSession, require_admin, require_admin_role},
};
use web_ui_common::guard::guard_failure;
use web_ui_common::shell_admin::AdminNavUser;

/// Admin sessions last 7 days, refreshed on use (same as the legacy portal).
const SESSION_TTL_SECONDS: i64 = 604_800;

/// The signed-in admin or host, or the redirect / HX-Redirect / 403 / 503.
pub async fn staff(cx: &Cx) -> topcoat::Result<AdminSession> {
    require_admin(cx).await.map_err(|e| guard_failure(cx, e))
}

/// The signed-in admin-role user (hosts get 403).
pub async fn admin_only(cx: &Cx) -> topcoat::Result<AdminSession> {
    require_admin_role(cx)
        .await
        .map_err(|e| guard_failure(cx, e))
}

/// The signed-in user for the shell, if any.
pub async fn nav_user(cx: &Cx) -> Option<AdminNavUser> {
    require_admin(cx).await.ok().map(|s| AdminNavUser {
        name: s.name,
        email: s.email,
        role: s.role,
    })
}

/// A short-lived token for calling the backend as `user`.
pub fn service_token(cx: &Cx, user: &AdminSession) -> topcoat::Result<String> {
    let jwt = try_app_context::<ServiceJwt>(cx)
        .ok_or_else(|| web_client::app_error("service JWT is not configured"))?;
    jwt.mint(user.user_id)
        .map_err(|e| web_client::app_error(e).into())
}

/// The portal role for a user's stored roles, if they may sign in here.
pub fn portal_role(user: &UserResponse) -> Option<AdminRole> {
    let has = |name: &str| user.roles.iter().any(|r| r.eq_ignore_ascii_case(name));
    if has("admin") || has("superadmin") {
        Some(AdminRole::Admin)
    } else if has("host") {
        Some(AdminRole::Host)
    } else {
        None
    }
}

/// Signs `user` in to the admin portal: ends any existing session (fixation
/// defence), starts a new one and stores it in the `admin` namespace.
pub async fn start_admin_session(
    cx: &Cx,
    user: &UserResponse,
    role: AdminRole,
) -> Result<(), String> {
    let api = get_api_client(cx);
    if let Some(old) = topcoat::session::stop(cx).await.ok().flatten() {
        let _ = api.delete_session(&token_hash_to_hex(&old)).await;
    }
    let session = topcoat::session::start(cx)
        .await
        .map_err(|e| format!("failed to start admin session: {e}"))?;
    let name = format!("{} {}", user.first_name, user.last_name)
        .trim()
        .to_string();
    let request = CreateSessionRequest {
        token_hash: token_hash_to_hex(&session.token_hash),
        user_id: user.id,
        email: user.email.clone(),
        name: if name.is_empty() {
            user.email.clone()
        } else {
            name
        },
        role: match role {
            AdminRole::Admin => "admin".to_string(),
            AdminRole::Host => "host".to_string(),
        },
        namespace: ADMIN_NAMESPACE.to_string(),
        ttl_seconds: SESSION_TTL_SECONDS,
    };
    if let Err(e) = api.create_session(&request).await {
        let _ = topcoat::session::stop(cx).await;
        return Err(format!("failed to store admin session: {e}"));
    }
    Ok(())
}

/// Listings the user may manage: all for admins, their own for hosts.
pub async fn scoped_listings(cx: &Cx, user: &AdminSession) -> Vec<ListingResponse> {
    let params = ListingSearchParams {
        per_page: Some(100),
        user_id: (user.role == AdminRole::Host).then_some(user.user_id),
        ..Default::default()
    };
    let listings = get_api_client(cx)
        .search_listings(params)
        .await
        .unwrap_or_default();
    match user.role {
        AdminRole::Admin => listings,
        // Defence in depth: never show a host another host's listing.
        AdminRole::Host => listings
            .into_iter()
            .filter(|l| l.user_id == user.user_id)
            .collect(),
    }
}

/// Bookings the user may see: all for admins, their listings' for hosts.
pub async fn scoped_bookings(
    cx: &Cx,
    user: &AdminSession,
    listings: &[ListingResponse],
) -> Vec<BookingResponse> {
    let all = get_api_client(cx)
        .get_all_bookings(Some(1), Some(100))
        .await
        .unwrap_or_default();
    match user.role {
        AdminRole::Admin => all,
        AdminRole::Host => {
            let own: HashSet<Uuid> = listings.iter().map(|l| l.id).collect();
            all.into_iter()
                .filter(|b| own.contains(&b.listing_id))
                .collect()
        }
    }
}

/// Escapes one CSV cell, and neutralises spreadsheet formulas
/// (`=`, `+`, `-`, `@` prefixes) so exported data cannot run in Excel/Sheets.
pub fn csv_cell(value: &str) -> String {
    let guarded = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::csv_cell;

    #[test]
    fn csv_cells_are_quoted_and_formula_safe() {
        assert_eq!(csv_cell("Villa \"A\""), "\"Villa \"\"A\"\"\"");
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_cell("-1"), "\"'-1\"");
        assert_eq!(csv_cell("ABC123"), "\"ABC123\"");
    }
}
