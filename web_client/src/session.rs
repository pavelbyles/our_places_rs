use common::models::SessionResponse;
use topcoat::context::{Cx, try_request_context};
use uuid::Uuid;

use crate::auth::token_hash_to_hex;
use crate::get_api_client;

/// Session namespace used by the guest portal.
pub const GUEST_NAMESPACE: &str = "guest";
/// Session namespace used by the admin portal.
pub const ADMIN_NAMESPACE: &str = "admin";

/// Why a route guard rejected a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GuardError {
    /// No valid session for this portal.
    #[error("authentication required")]
    Unauthenticated,
    /// Signed in, but the role is not allowed here.
    #[error("forbidden")]
    Forbidden,
    /// The session service could not be reached.
    #[error("session service unavailable")]
    Service,
}

/// Role of a user signed in to the admin portal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminRole {
    /// `admin` or `superadmin`: full access.
    Admin,
    /// `host`: access scoped to the host's own listings by the backend.
    Host,
}

impl AdminRole {
    /// Maps a stored role name; `None` for roles with no admin-portal access.
    pub fn from_role(role: &str) -> Option<Self> {
        match role.to_ascii_lowercase().as_str() {
            "admin" | "superadmin" => Some(Self::Admin),
            "host" => Some(Self::Host),
            _ => None,
        }
    }
}

/// A signed-in guest. The user ID is always present and never nil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestSession {
    /// The guest's user ID.
    pub user_id: Uuid,
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
}

impl GuestSession {
    /// Validates a stored session for the guest portal.
    pub fn from_session(session: &SessionResponse) -> Result<Self, GuardError> {
        valid_user(session, GUEST_NAMESPACE).map(|user_id| Self {
            user_id,
            name: session.name.clone(),
            email: session.email.clone(),
        })
    }
}

/// A signed-in admin-portal user. The user ID is always present and never nil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminSession {
    /// The user's ID.
    pub user_id: Uuid,
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
    /// Admin-portal role.
    pub role: AdminRole,
}

impl AdminSession {
    /// Validates a stored session for the admin portal.
    pub fn from_session(session: &SessionResponse) -> Result<Self, GuardError> {
        let user_id = valid_user(session, ADMIN_NAMESPACE)?;
        let role = AdminRole::from_role(&session.role).ok_or(GuardError::Forbidden)?;
        Ok(Self {
            user_id,
            name: session.name.clone(),
            email: session.email.clone(),
            role,
        })
    }

    /// Passes only users with the [`AdminRole::Admin`] role.
    pub fn require_admin_role(self) -> Result<Self, GuardError> {
        match self.role {
            AdminRole::Admin => Ok(self),
            AdminRole::Host => Err(GuardError::Forbidden),
        }
    }
}

fn valid_user(session: &SessionResponse, namespace: &str) -> Result<Uuid, GuardError> {
    Some(session.user_id)
        .filter(|id| !id.is_nil() && session.namespace == namespace)
        .ok_or(GuardError::Unauthenticated)
}

/// Looks up the current request's session in `namespace`, refreshing its TTL.
async fn current_session(cx: &Cx, namespace: &str) -> Result<SessionResponse, GuardError> {
    try_request_context::<topcoat::session::SessionState>(cx).ok_or(GuardError::Unauthenticated)?;
    let token_hash = topcoat::session::token_hash(cx)
        .await
        .map_err(|_| GuardError::Service)?
        .ok_or(GuardError::Unauthenticated)?;
    let session = get_api_client(cx)
        .get_session(&token_hash_to_hex(&token_hash), Some(namespace))
        .await
        .map_err(|_| GuardError::Service)?
        .ok_or(GuardError::Unauthenticated)?;
    // Sliding expiration: a failed cookie refresh must not fail the request.
    if let Err(e) = topcoat::session::refresh(cx).await {
        tracing::warn!("Failed to refresh session cookie: {e}");
    }
    Ok(session)
}

/// Requires a signed-in guest.
pub async fn require_guest(cx: &Cx) -> Result<GuestSession, GuardError> {
    current_session(cx, GUEST_NAMESPACE)
        .await
        .and_then(|s| GuestSession::from_session(&s))
}

/// Requires a signed-in admin or host.
pub async fn require_admin(cx: &Cx) -> Result<AdminSession, GuardError> {
    current_session(cx, ADMIN_NAMESPACE)
        .await
        .and_then(|s| AdminSession::from_session(&s))
}

/// Requires a signed-in user with the admin role (hosts are forbidden).
pub async fn require_admin_role(cx: &Cx) -> Result<AdminSession, GuardError> {
    require_admin(cx)
        .await
        .and_then(AdminSession::require_admin_role)
}

/// Login URL that returns to `next` after signing in.
pub fn login_redirect(next: &str) -> String {
    format!("/login?redirect={}", encode_query_component(next))
}

/// Percent-encodes everything except RFC 3986 unreserved characters.
fn encode_query_component(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
