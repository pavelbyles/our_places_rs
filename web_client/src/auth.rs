use serde::{Deserialize, Serialize};
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

/// Helper to convert TokenHash to standard 64-char lowercase hex string.
pub fn token_hash_to_hex(hash: &topcoat::session::TokenHash) -> String {
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}
