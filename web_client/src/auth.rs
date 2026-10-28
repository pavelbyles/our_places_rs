use crate::get_api_client;
use serde::{Deserialize, Serialize};
use topcoat::context::{Cx, try_request_context};
use uuid::Uuid;

/// Strongly-typed authenticated user context for server-side rendering (SSR).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthUser {
    /// User ID from the session, when known.
    pub id: Option<Uuid>,
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
    /// Primary role (`guest`, `host`, `admin`, `superadmin`).
    pub role: String,
    /// Session namespace (`guest` or `admin`).
    pub namespace: String,
}

impl AuthUser {
    /// Builds a `guest`-namespace user without an ID.
    pub fn new(name: impl Into<String>, email: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            id: None,
            name: name.into(),
            email: email.into(),
            role: role.into(),
            namespace: "guest".to_string(),
        }
    }

    /// Builds a user in the given session namespace, without an ID.
    pub fn with_namespace(
        name: impl Into<String>,
        email: impl Into<String>,
        role: impl Into<String>,
        namespace: impl Into<String>,
    ) -> Self {
        Self {
            id: None,
            name: name.into(),
            email: email.into(),
            role: role.into(),
            namespace: namespace.into(),
        }
    }

    /// Computes initials in pure Rust (e.g. "David Sterling" -> "DS", "Elena" -> "EL").
    pub fn initials(&self) -> String {
        let parts: Vec<&str> = self.name.split_whitespace().collect();
        match parts.as_slice() {
            [first, last, ..] => {
                let f = first.chars().next().unwrap_or('U');
                let l = last.chars().next().unwrap_or(' ');
                format!("{}{}", f, l).trim().to_uppercase()
            }
            [single] => {
                let chars: String = single.chars().take(2).collect();
                if chars.is_empty() {
                    "U".to_string()
                } else {
                    chars.to_uppercase()
                }
            }
            [] => "U".to_string(),
        }
    }

    /// Returns human-readable role badge title.
    pub fn role_display(&self) -> &str {
        match self.role.to_lowercase().as_str() {
            "admin" | "superadmin" => "Administrator",
            "host" => "Host",
            _ => "Verified Guest",
        }
    }

    /// Whether the role is `admin` or `superadmin`.
    pub fn is_admin(&self) -> bool {
        self.role.eq_ignore_ascii_case("admin") || self.role.eq_ignore_ascii_case("superadmin")
    }

    /// Whether the role is `host`.
    pub fn is_host(&self) -> bool {
        self.role.eq_ignore_ascii_case("host")
    }

    /// Whether the user may sign in to the admin portal (admin or host).
    pub fn is_authorized_for_admin_portal(&self) -> bool {
        self.is_admin() || self.is_host()
    }
}

/// Why an admin-portal route guard rejected a request.
#[derive(Debug, thiserror::Error)]
pub enum AdminAuthError {
    /// No valid admin session; carries the login path to redirect to.
    #[error("Authentication required")]
    Unauthenticated(String),

    /// Signed in, but the role is not allowed; carries the reason.
    #[error("Forbidden: Insufficient privileges for admin portal")]
    Forbidden(String),

    /// The session service failed.
    #[error("Service error: {0}")]
    Service(String),
}

/// Helper to convert TokenHash to standard 64-char lowercase hex string.
pub fn token_hash_to_hex(hash: &topcoat::session::TokenHash) -> String {
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Fetches and touches active admin session, refreshing client cookie and extending DB TTL.
pub async fn get_admin_session(cx: &Cx) -> Option<AuthUser> {
    try_request_context::<topcoat::session::SessionState>(cx)?;
    if let Ok(Some(token_hash)) = topcoat::session::token_hash(cx).await {
        let hash_hex = token_hash_to_hex(&token_hash);
        let api = get_api_client(cx);
        if let Ok(Some(session)) = api.get_session(&hash_hex, Some("admin")).await {
            // Sliding expiration: refresh client cookie in Topcoat
            let _ = topcoat::session::refresh(cx).await;
            return Some(AuthUser {
                id: Some(session.user_id),
                name: session.name,
                email: session.email,
                role: session.role,
                namespace: session.namespace,
            });
        }
    }
    None
}

/// Fetches and touches active guest session, refreshing client cookie and extending DB TTL.
pub async fn get_guest_session(cx: &Cx) -> Option<AuthUser> {
    try_request_context::<topcoat::session::SessionState>(cx)?;
    if let Ok(Some(token_hash)) = topcoat::session::token_hash(cx).await {
        let hash_hex = token_hash_to_hex(&token_hash);
        let api = get_api_client(cx);
        if let Ok(Some(session)) = api.get_session(&hash_hex, Some("guest")).await {
            // Sliding expiration: refresh client cookie in Topcoat
            let _ = topcoat::session::refresh(cx).await;
            return Some(AuthUser {
                id: Some(session.user_id),
                name: session.name,
                email: session.email,
                role: session.role,
                namespace: session.namespace,
            });
        }
    }
    None
}

/// Route guard: requires active admin session.
pub async fn require_admin_auth(cx: &Cx) -> Result<AuthUser, AdminAuthError> {
    match get_admin_session(cx).await {
        Some(user) => {
            if user.is_authorized_for_admin_portal() {
                Ok(user)
            } else {
                Err(AdminAuthError::Forbidden(
                    "User does not possess administrative privileges".to_string(),
                ))
            }
        }
        None => Err(AdminAuthError::Unauthenticated("/login".to_string())),
    }
}

/// Route guard: requires active admin session WITH Admin role strictly (excludes Host).
pub async fn require_admin_role_auth(cx: &Cx) -> Result<AuthUser, AdminAuthError> {
    match get_admin_session(cx).await {
        Some(user) => {
            if user.is_admin() {
                Ok(user)
            } else {
                Err(AdminAuthError::Forbidden(
                    "User does not possess administrative privileges to manage users".to_string(),
                ))
            }
        }
        None => Err(AdminAuthError::Unauthenticated("/login".to_string())),
    }
}

/// Legacy placeholder used by `web_app_admin_tc`: always returns a fixed
/// administrator. New crates must use the session guards instead.
pub fn get_authenticated_admin(_cx: &Cx) -> AuthUser {
    AuthUser::with_namespace("Administrator", "admin@ourplaces.io", "admin", "admin")
}

/// Legacy placeholder used by `web_app_tc`: always returns `None`. New crates
/// must use [`get_guest_session`] or the session guards instead.
pub fn get_authenticated_guest(_cx: &Cx) -> Option<AuthUser> {
    None
}
