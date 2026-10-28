//! Guest and admin authentication. The Rust session logic lives in
//! [`web_client::auth`] and is re-exported here at its original path; the
//! legacy client-side auth script stays in this crate.

pub use web_client::auth::*;

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
