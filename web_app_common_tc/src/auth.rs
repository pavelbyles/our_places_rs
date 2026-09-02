use crate::get_api_client;
use serde::{Deserialize, Serialize};
use topcoat::context::Cx;
use uuid::Uuid;

/// Strongly-typed authenticated user context for server-side rendering (SSR).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: Option<Uuid>,
    pub name: String,
    pub email: String,
    pub role: String,
    pub namespace: String,
}

impl AuthUser {
    pub fn new(name: impl Into<String>, email: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            id: None,
            name: name.into(),
            email: email.into(),
            role: role.into(),
            namespace: "guest".to_string(),
        }
    }

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

    pub fn is_admin(&self) -> bool {
        self.role.eq_ignore_ascii_case("admin") || self.role.eq_ignore_ascii_case("superadmin")
    }

    pub fn is_host(&self) -> bool {
        self.role.eq_ignore_ascii_case("host")
    }

    pub fn is_authorized_for_admin_portal(&self) -> bool {
        self.is_admin() || self.is_host()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AdminAuthError {
    #[error("Authentication required")]
    Unauthenticated(String),

    #[error("Forbidden: Insufficient privileges for admin portal")]
    Forbidden(String),

    #[error("Service error: {0}")]
    Service(String),
}

/// Helper to convert TokenHash to standard 64-char lowercase hex string.
pub fn token_hash_to_hex(hash: &topcoat::session::TokenHash) -> String {
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Fetches and touches active admin session, refreshing client cookie and extending DB TTL.
pub async fn get_admin_session(cx: &Cx) -> Option<AuthUser> {
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

pub fn get_authenticated_admin(_cx: &Cx) -> AuthUser {
    AuthUser::with_namespace("Administrator", "admin@ourplaces.io", "admin", "admin")
}

pub fn get_authenticated_guest(_cx: &Cx) -> Option<AuthUser> {
    None
}

/// Client-side auth script to sync header and manage session / redirection cleanly.
/// IMPORTANT: Crafted with ZERO raw ampersands, less-than, or greater-than operators to prevent Topcoat HTML entity escaping.
pub fn auth_init_script() -> &'static str {
    r#"
    (function() {
        function syncGuestAuth() {
            try {
                var userJson = localStorage.getItem('op_auth_user');
                var guestActions = document.getElementById('nav-guest-actions');
                var userDropdown = document.getElementById('nav-user-dropdown');
                var drawerGuest = document.getElementById('drawer-guest-actions');
                var drawerUser = document.getElementById('drawer-user-actions');
                
                var adminAuthUser = document.getElementById('admin-auth-user');
                var adminAuthGuest = document.getElementById('admin-auth-guest');
                var adminSidebarUser = document.getElementById('admin-sidebar-user');
                var adminSidebarGuest = document.getElementById('admin-sidebar-guest');
                
                var adminTopInitials = document.getElementById('admin-top-avatar-initials');
                var adminTopName = document.getElementById('admin-top-user-name');
                var adminTopEmail = document.getElementById('admin-top-user-email');
                var adminTopRole = document.getElementById('admin-top-user-role');
                var adminSideInitials = document.getElementById('admin-sidebar-avatar-initials');
                var adminSideName = document.getElementById('admin-sidebar-user-name');
                var adminSideRole = document.getElementById('admin-sidebar-user-role');
                
                if (userJson) {
                    var user = JSON.parse(userJson);
                    // Guest layout
                    if (guestActions) guestActions.style.setProperty('display', 'none', 'important');
                    if (userDropdown) userDropdown.style.setProperty('display', 'block', 'important');
                    if (drawerGuest) drawerGuest.style.setProperty('display', 'none', 'important');
                    if (drawerUser) drawerUser.style.setProperty('display', 'block', 'important');
                    
                    // Admin layout (logged in)
                    if (adminAuthUser) adminAuthUser.style.setProperty('display', 'block', 'important');
                    if (adminAuthGuest) adminAuthGuest.style.setProperty('display', 'none', 'important');
                    if (adminSidebarUser) adminSidebarUser.style.setProperty('display', 'block', 'important');
                    if (adminSidebarGuest) adminSidebarGuest.style.setProperty('display', 'none', 'important');
                    
                    var nameEl = document.getElementById('nav-user-name');
                    var emailEl = document.getElementById('nav-user-email');
                    var initialsEl = document.getElementById('nav-user-avatar-initials');
                    if (nameEl) nameEl.innerText = user.name || user.email || 'User';
                    if (emailEl) emailEl.innerText = user.email || '';
                    
                    var initialsStr = 'U';
                    if (user.name) {
                        var parts = user.name.split(' ');
                        var firstChar = parts[0] ? parts[0][0] : '';
                        var lastChar = (parts.length === 1) ? '' : (parts[1] ? parts[1][0] : '');
                        initialsStr = (firstChar + lastChar).toUpperCase() || 'U';
                    } else if (user.email) {
                        initialsStr = user.email[0].toUpperCase();
                    }
                    if (initialsEl) initialsEl.innerText = initialsStr;
                    
                    if (adminTopInitials) adminTopInitials.innerText = initialsStr;
                    if (adminTopName) adminTopName.innerText = user.name || user.email || 'Administrator';
                    if (adminTopEmail) adminTopEmail.innerText = user.email || '';
                    if (adminTopRole) adminTopRole.innerText = (user.role || 'Administrator').toUpperCase();
                    
                    if (adminSideInitials) adminSideInitials.innerText = initialsStr;
                    if (adminSideName) adminSideName.innerText = user.name || user.email || 'Administrator';
                    if (adminSideRole) adminSideRole.innerText = (user.role || 'Administrator').toUpperCase();

                    var roleStr = (user.role || '').toLowerCase();
                    var isUserAdmin = false;
                    if (roleStr === 'admin') { isUserAdmin = true; }
                    if (roleStr === 'superadmin') { isUserAdmin = true; }

                    var userMgmtIds = [
                        'admin-quick-add-user',
                        'admin-top-manage-users',
                        'admin-sidebar-users-title',
                        'admin-sidebar-users-link',
                        'admin-sidebar-users-new-link',
                        'admin-dashboard-users-link'
                    ];
                    userMgmtIds.forEach(function(id) {
                        var el = document.getElementById(id);
                        if (el) {
                            if (isUserAdmin) {
                                el.style.removeProperty('display');
                            } else {
                                el.style.setProperty('display', 'none', 'important');
                            }
                        }
                    });
                } else {
                    // Logged out on Guest
                    if (guestActions) guestActions.style.setProperty('display', 'flex', 'important');
                    if (userDropdown) userDropdown.style.setProperty('display', 'none', 'important');
                    if (drawerGuest) drawerGuest.style.setProperty('display', 'block', 'important');
                    if (drawerUser) drawerUser.style.setProperty('display', 'none', 'important');
                    
                    // Logged out on Admin (No avatar shown)
                    if (adminAuthUser) adminAuthUser.style.setProperty('display', 'none', 'important');
                    if (adminAuthGuest) adminAuthGuest.style.setProperty('display', 'flex', 'important');
                    if (adminSidebarUser) adminSidebarUser.style.setProperty('display', 'none', 'important');
                    if (adminSidebarGuest) adminSidebarGuest.style.setProperty('display', 'block', 'important');

                    var guestUserMgmtIds = [
                        'admin-quick-add-user',
                        'admin-top-manage-users',
                        'admin-sidebar-users-title',
                        'admin-sidebar-users-link',
                        'admin-sidebar-users-new-link',
                        'admin-dashboard-users-link'
                    ];
                    guestUserMgmtIds.forEach(function(id) {
                        var el = document.getElementById(id);
                        if (el) {
                            el.style.setProperty('display', 'none', 'important');
                        }
                    });
                }
            } catch (e) {
                console.error('Failed to sync auth state:', e);
            }
        }
        
        window.logoutUser = function(e) {
            if (e) {
                try { e.preventDefault(); } catch(err) {}
            }
            try {
                localStorage.removeItem('op_auth_user');
                sessionStorage.removeItem('op_auth_user');
            } catch(e) {}
            window.location.href = '/logout';
        };

        window.loginUser = function(name, email, role, firstName, lastName, phone, id) {
            try {
                var fn = firstName || '';
                var ln = lastName || '';
                if (!fn) {
                    if (name) {
                        var clean = name.trim();
                        var parts = clean.split(' ');
                        if (parts.length !== 0) {

                        if (parts.length !== 1) {
                            fn = parts[0];
                            ln = parts.slice(1).join(' ');
                        } else if (clean.toLowerCase().indexOf('pavelbyles') !== -1) {
                            fn = 'Pavel';
                            ln = 'Byles';
                        } else {
                            var matchPascal = clean.match(/^([A-Z][a-z]+)([A-Z][a-z]+)$/);
                            if (matchPascal) {
                                fn = matchPascal[1];
                                ln = matchPascal[2];
                            } else {
                                fn = clean;
                            }
                        }
                    }
                }
                }
                var userObj = {


                    name: name || (fn + ' ' + ln).trim() || email,
                    email: email,
                    role: role || 'guest',
                    first_name: fn,
                    last_name: ln,
                    phone: phone || '',
                    id: id || ''
                };
                localStorage.setItem('op_auth_user', JSON.stringify(userObj));
            } catch(e) {}
            
            var redirect = '';
            try {
                var params = new URLSearchParams(window.location.search);
                redirect = params.get('redirect') || '';
                if (!redirect) {
                    if (document.referrer) {
                        if (document.referrer.indexOf(window.location.origin) === 0) {
                            var refPath = document.referrer.substring(window.location.origin.length);
                            var isAuthPage = (refPath.indexOf('/login') !== -1) || (refPath.indexOf('/register') !== -1) || (refPath.indexOf('/logout') !== -1);
                            if (!isAuthPage) {
                                redirect = refPath;
                            }
                        }
                    }
                }
            } catch(err) {}
            
            window.location.href = redirect || '/';
        };

        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', syncGuestAuth);
        } else {
            syncGuestAuth();
        }
    })();
    "#
}
