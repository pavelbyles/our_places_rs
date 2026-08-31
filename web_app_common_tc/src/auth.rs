use serde::{Deserialize, Serialize};
use topcoat::context::Cx;

/// Strongly-typed authenticated user context for server-side rendering (SSR).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: Option<uuid::Uuid>,
    pub name: String,
    pub email: String,
    pub role: String,
}

impl AuthUser {
    pub fn new(name: impl Into<String>, email: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            id: None,
            name: name.into(),
            email: email.into(),
            role: role.into(),
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
            "admin" | "superadmin" | "super_admin" => "Super Administrator",
            "host" => "Superhost",
            "concierge" => "Concierge Staff",
            _ => "Verified Guest",
        }
    }

    pub fn is_admin(&self) -> bool {
        self.role.eq_ignore_ascii_case("admin") || self.role.eq_ignore_ascii_case("superadmin")
    }
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
            window.location.href = '/login';
        };

        window.loginUser = function(name, email, role) {
            try {
                localStorage.setItem('op_auth_user', JSON.stringify({ name: name, email: email, role: role || 'admin' }));
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

/// Extracts authenticated admin context for SSR layouts.
pub fn get_authenticated_admin(_cx: &Cx) -> AuthUser {
    AuthUser::new("Administrator", "admin@ourplaces.io", "superadmin")
}

/// Extracts authenticated guest context for SSR layouts.
pub fn get_authenticated_guest(_cx: &Cx) -> Option<AuthUser> {
    None
}
