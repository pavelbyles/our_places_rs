use serde::Deserialize;
use topcoat::context::{Cx, try_app_context};
use topcoat::router::parse_query_params;
use web_client::jwt::ServiceJwt;
use web_client::session::{GuestSession, require_guest};
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
