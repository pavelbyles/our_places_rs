use common::models::{CreateSessionRequest, UpdateUserRequest, UserResponse};
use serde::Deserialize;
use topcoat::context::{Cx, try_context};
use topcoat::cookie::{Cookie, Cookies, SameSite, cookies, time::Duration};
use topcoat::router::parse_query_params;
use topcoat::router::request::{headers, uri};
use web_client::auth::token_hash_to_hex;
use web_client::get_api_client;
use web_client::jwt::ServiceJwt;
use web_client::session::{GUEST_NAMESPACE, GuestSession, require_guest};
use web_ui_common::components::domain::currency_selector::{
    CURRENCY_COOKIE, active_currency_codes,
};
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

/// The guest's display currency: `?currency=` when the URL has one (shared
/// and checkout links), else the one they chose last (the
/// [`CURRENCY_COOKIE`]), else a signed-in guest's profile default, if it is
/// currently offered.
///
/// A code that is not in the active list (e.g. deactivated since it was
/// chosen) is dropped, so pages show prices in their base currency (correctly
/// labelled) and the selector asks the guest to choose. When the list cannot
/// be loaded the code is passed through and the API validates it.
pub async fn display_currency(cx: &Cx) -> Option<String> {
    let from_url = parse_query_params::<CurrencyQuery>(cx)
        .ok()
        .and_then(|q| q.currency);
    let from_cookie = || {
        cookies(cx)
            .get(CURRENCY_COOKIE)
            .map(|c| c.value().to_string())
    };
    let requested = match normalise_code(from_url).or_else(|| normalise_code(from_cookie())) {
        Some(code) => code,
        None => normalise_code(profile_currency(cx).await)?,
    };
    let offered = active_currency_codes(cx).await;
    (offered.is_empty() || offered.contains(&requested)).then_some(requested)
}

/// A signed-in guest's profile default currency. Only consulted when there
/// is no cookie, which signing in sets, so it is rarely fetched.
async fn profile_currency(cx: &Cx) -> Option<String> {
    let session = require_guest(cx).await.ok()?;
    match get_api_client(cx).get_user_profile(session.user_id).await {
        Ok(user) => Some(user.default_currency),
        Err(e) => {
            tracing::warn!(user_id = %session.user_id, error = %e, "profile currency not fetched; showing base prices");
            None
        }
    }
}

/// How long a display-currency choice is remembered.
const REMEMBER_CURRENCY_FOR: Duration = Duration::days(365);

/// Whether the request reached us over HTTPS (directly or via Cloud Run's
/// proxy), so cookies are marked `Secure`.
fn is_https(cx: &Cx) -> bool {
    uri(cx).scheme_str() == Some("https")
        || headers(cx)
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|proto| proto.eq_ignore_ascii_case("https"))
}

/// Remembers `code` as the display currency (the [`CURRENCY_COOKIE`]). Call
/// only from handlers, before the response is written.
pub fn remember_currency(cx: &Cx, code: &str) {
    let cookie = Cookie::build((CURRENCY_COOKIE, code.to_string()))
        .path("/")
        .max_age(REMEMBER_CURRENCY_FOR)
        .same_site(SameSite::Lax)
        .http_only(true)
        .secure(is_https(cx));
    cookies(cx).add(cookie);
}

/// Saves `code` as the guest's profile default currency (only that field
/// changes). The display choice is already remembered in the cookie, so a
/// failure here is logged and the guest is not interrupted.
pub async fn save_profile_currency(cx: &Cx, user_id: uuid::Uuid, code: &str) {
    let request = UpdateUserRequest {
        first_name: None,
        last_name: None,
        phone_number: None,
        default_currency: Some(code.to_string()),
        email: None,
        password: None,
        is_active: None,
        is_verified: None,
        attributes: None,
        roles: None,
        booker_profile: None,
        host_profile: None,
        slug: None,
    };
    if let Err(e) = get_api_client(cx).update_user(user_id, &request).await {
        tracing::warn!(%user_id, currency = code, error = %e, "display currency not saved to the profile");
    }
}

/// An upper-cased currency code, or `None` if blank.
fn normalise_code(code: Option<String>) -> Option<String> {
    code.map(|c| c.trim().to_uppercase())
        .filter(|c| !c.is_empty())
}

/// The signed-in guest, or the redirect / HX-Redirect / 403 / 503 response.
pub async fn guest(cx: &Cx) -> topcoat::Result<GuestSession> {
    require_guest(cx).await.map_err(|e| guard_failure(cx, e))
}

/// A short-lived token for calling the backend as `guest`.
pub fn service_token(cx: &Cx, guest: &GuestSession) -> topcoat::Result<String> {
    let jwt = try_context::<ServiceJwt>(cx)
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
    // Their profile currency applies unless they already chose one here.
    if cookies(cx).get(CURRENCY_COOKIE).is_none()
        && let Some(code) = normalise_code(Some(user.default_currency.clone()))
    {
        remember_currency(cx, &code);
    }
    Ok(())
}

/// The party size when a guest hasn't given one.
pub const DEFAULT_GUESTS: u8 = 1;

/// The guest count given, or the default. Pages read it as `i32` or `u32`.
pub fn guests_or_default<T: From<u8>>(guests: Option<T>) -> T {
    guests.unwrap_or_else(|| T::from(DEFAULT_GUESTS))
}

/// Today's date in Jamaica (UTC-05:00, no DST): the listings' local day.
pub fn jamaica_today() -> chrono::NaiveDate {
    common::availability::jamaica_today(chrono::Utc::now())
}

/// Loads booking `id` as `guest`. Anything the guest does not own (or that
/// does not exist) is a 404, so IDs cannot be probed.
pub async fn owned_booking(
    cx: &Cx,
    guest: &GuestSession,
    id: uuid::Uuid,
) -> topcoat::Result<common::models::BookingResponse> {
    let token = service_token(cx, guest)?;
    match get_api_client(cx).get_booking_as(id, &token).await {
        Ok(b) if b.guest_id == guest.user_id => Ok(b),
        Ok(_)
        | Err(web_client::client::UserCallError::NotFound)
        | Err(web_client::client::UserCallError::Unauthorized) => {
            tracing::info!(booking_id = %id, "booking access denied");
            Err(topcoat::router::error::not_found().into())
        }
        Err(e) => Err(web_client::app_error(e).into()),
    }
}

/// Parses a booking ID path segment; malformed IDs are a 404.
pub fn parse_booking_id(raw: &str) -> topcoat::Result<uuid::Uuid> {
    uuid::Uuid::parse_str(raw).map_err(|_| topcoat::router::error::not_found().into())
}

#[cfg(test)]
mod default_guests_tests {
    use super::guests_or_default;

    #[test]
    fn no_guest_count_means_one_guest() {
        assert_eq!(guests_or_default::<i32>(None), 1);
        assert_eq!(guests_or_default::<u32>(None), 1);
    }

    #[test]
    fn a_guest_count_given_is_kept() {
        assert_eq!(guests_or_default(Some(3_i32)), 3);
        assert_eq!(guests_or_default(Some(3_u32)), 3);
    }
}
