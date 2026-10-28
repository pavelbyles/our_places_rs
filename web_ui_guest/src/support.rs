use common::models::{CreateSessionRequest, UserResponse};
use serde::Deserialize;
use topcoat::context::{Cx, try_app_context};
use topcoat::router::parse_query_params;
use web_client::auth::token_hash_to_hex;
use web_client::get_api_client;
use web_client::jwt::ServiceJwt;
use web_client::session::{GUEST_NAMESPACE, GuestSession, require_guest};
use web_ui_common::guard::guard_failure;
use web_ui_common::shell_guest::NavUser;

#[derive(Deserialize, Default)]
struct CurrencyQuery {
    currency: Option<String>,
}

/// The signed-in guest for the header, if any (one lookup per request).
pub async fn nav_user(cx: &Cx) -> Option<NavUser> {
    require_guest(cx).await.ok().map(|s| NavUser {
        name: s.name,
        email: s.email,
    })
}

/// The `?currency=` display currency, if given.
pub fn display_currency(cx: &Cx) -> Option<String> {
    parse_query_params::<CurrencyQuery>(cx)
        .ok()
        .and_then(|q| q.currency)
        .filter(|c| !c.is_empty())
}

/// The signed-in guest, or the redirect / HX-Redirect / 403 / 503 response.
pub async fn guest(cx: &Cx) -> topcoat::Result<GuestSession> {
    require_guest(cx).await.map_err(|e| guard_failure(cx, e))
}

/// A short-lived token for calling the backend as `guest`.
pub fn service_token(cx: &Cx, guest: &GuestSession) -> topcoat::Result<String> {
    let jwt = try_app_context::<ServiceJwt>(cx)
        .ok_or_else(|| web_client::app_error("service JWT is not configured"))?;
    jwt.mint(guest.user_id)
        .map_err(|e| web_client::app_error(e).into())
}

/// Guest sessions last 7 days, refreshed on use (same as the legacy portal).
const SESSION_TTL_SECONDS: i64 = 604_800;

/// Signs `user` in: ends any existing session (fixation defence), starts a
/// new one and stores it with `user_api` in the `guest` namespace.
pub async fn start_guest_session(cx: &Cx, user: &UserResponse) -> Result<(), String> {
    let api = get_api_client(cx);
    if let Some(old) = topcoat::session::stop(cx).await.ok().flatten() {
        let _ = api.delete_session(&token_hash_to_hex(&old)).await;
    }
    let session = topcoat::session::start(cx)
        .await
        .map_err(|e| format!("failed to start guest session: {e}"))?;
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
        role: "guest".to_string(),
        namespace: GUEST_NAMESPACE.to_string(),
        ttl_seconds: SESSION_TTL_SECONDS,
    };
    if let Err(e) = api.create_session(&request).await {
        let _ = topcoat::session::stop(cx).await;
        return Err(format!("failed to store guest session: {e}"));
    }
    Ok(())
}

/// Today's date in Jamaica (UTC-05:00, no DST): the listings' local day.
pub fn jamaica_today() -> chrono::NaiveDate {
    (chrono::Utc::now() - chrono::Duration::hours(5)).date_naive()
}
