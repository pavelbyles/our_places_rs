//! Turns a failed route guard into the right HTTP response.

use topcoat::context::Cx;
use topcoat::router::error::{forbidden, see_other, service_unavailable, unauthorized};
use topcoat::router::header::{HeaderName, HeaderValue};
use topcoat::router::request::uri;
use topcoat::router::response::response_headers;
use web_client::session::{GuardError, login_redirect};

/// The current request's path and query, for a post-login return.
pub fn current_path(cx: &Cx) -> String {
    uri(cx)
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| "/".to_string())
}

/// Maps a guard failure to a response error:
/// - not signed in: 303 to `/login?redirect=<this page>`, or for an HTMX
///   request a 401 carrying `HX-Redirect` (so a login page is never swapped
///   into a fragment target);
/// - wrong role: 403;
/// - session service down: 503.
pub fn guard_failure(cx: &Cx, error: GuardError) -> topcoat::Error {
    match error {
        GuardError::Unauthenticated => {
            let target = login_redirect(&current_path(cx));
            if topcoat::htmx::hx_request(cx) {
                if let Ok(value) = HeaderValue::from_str(&target) {
                    response_headers(cx).append(HeaderName::from_static("hx-redirect"), value);
                }
                unauthorized().into()
            } else {
                see_other(target).into()
            }
        }
        GuardError::Forbidden => forbidden().into(),
        GuardError::Service => service_unavailable(5).into(),
    }
}
