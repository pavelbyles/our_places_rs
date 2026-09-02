use common::models::{CreateSessionRequest, LoginRequest, UserResponse};
use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, page, route},
    view::view,
};
use web_app_common_tc::{auth::token_hash_to_hex, get_api_client};

#[derive(Serialize, Deserialize)]
pub struct AdminLoginResponse {
    pub success: bool,
    pub user: Option<UserResponse>,
    pub message: Option<String>,
}

#[route(POST "/api/auth/login")]
pub async fn admin_login_api(
    cx: &Cx,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<AdminLoginResponse>> {
    let api = get_api_client(cx);
    
    // Authenticate credentials against user_api
    match api.login_user(&payload).await {
        Ok(user_resp) => {
            // Verify roles: user must be an admin, superadmin, or host
            let is_authorized = user_resp.roles.iter().any(|r| {
                let lr = r.to_lowercase();
                lr == "admin" || lr == "superadmin" || lr == "host"
            });

            if !is_authorized {
                return Ok(Json(AdminLoginResponse {
                    success: false,
                    user: None,
                    message: Some("Access denied. Admin or host role required.".to_string()),
                }));
            }

            // Start Topcoat session (issues op_admin_session cookie to client)
            match topcoat::session::start(cx).await {
                Ok(session) => {
                    let hash_hex = token_hash_to_hex(&session.token_hash);
                    let primary_role = user_resp

                        .roles
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "admin".to_string());
                    let full_name = format!("{} {}", user_resp.first_name, user_resp.last_name)
                        .trim()
                        .to_string();

                    // Persist session into user_api with namespace: "admin"
                    let create_req = CreateSessionRequest {
                        token_hash: hash_hex,
                        user_id: user_resp.id,
                        email: user_resp.email.clone(),
                        name: if full_name.is_empty() {
                            user_resp.email.clone()
                        } else {
                            full_name
                        },
                        role: primary_role,
                        namespace: "admin".to_string(),
                        ttl_seconds: 604800, // 7-day TTL with rolling refresh
                    };

                    if let Err(e) = api.create_session(&create_req).await {
                        tracing::error!("Failed to persist admin session in user_api: {:?}", e);
                        return Ok(Json(AdminLoginResponse {
                            success: false,
                            user: None,
                            message: Some("Failed to persist session. Please try again.".to_string()),
                        }));
                    }

                    Ok(Json(AdminLoginResponse {
                        success: true,
                        user: Some(user_resp),
                        message: None,
                    }))
                }
                Err(e) => {
                    tracing::error!("Failed to start Topcoat session: {:?}", e);
                    Ok(Json(AdminLoginResponse {
                        success: false,
                        user: None,
                        message: Some("Failed to initialize server session".to_string()),
                    }))
                }
            }
        }
        Err(e) => {
            tracing::warn!("Login failed for email {}: {:?}", payload.email, e);
            Ok(Json(AdminLoginResponse {
                success: false,
                user: None,
                message: Some("Invalid email or password. Access restricted to authorized administrators.".to_string()),
            }))
        }
    }
}

#[page("/logout")]
pub async fn logout_page(cx: &Cx) -> Result {
    if let Ok(Some(token_hash)) = topcoat::session::token_hash(cx).await {
        let hash_hex = token_hash_to_hex(&token_hash);
        let api = get_api_client(cx);
        let _ = api.delete_session(&hash_hex).await;
    }
    let _ = topcoat::session::stop(cx).await;

    view! {
        <div class="min-h-[60vh] flex items-center justify-center">
            <div class="text-center space-y-4">
                <span class="loading loading-spinner loading-lg text-primary"></span>
                <p class="text-sm font-medium text-base-content/70">"Signing out of Executive Console..."</p>
            </div>
        </div>
        <script>
            r#"
            try {
                localStorage.removeItem('op_auth_user');
                sessionStorage.removeItem('op_auth_user');
            } catch(e) {}
            window.location.replace('/login');
            "#
        </script>
    }
}

#[page("/login")]
pub async fn login_page(cx: &Cx) -> Result {
    // If user already has a valid active admin session, redirect straight to dashboard
    if let Some(_user) = web_app_common_tc::auth::get_admin_session(cx).await {
        return view! {
            <script>
                r#"
                var params = new URLSearchParams(window.location.search);
                var target = params.get('redirect') || '/admin';
                window.location.replace(target);
                "#
            </script>
        };
    }

    view! {
        <div class="min-h-[75vh] flex items-center justify-center py-12 px-4 sm:px-6 lg:px-8">
            <div class="w-full max-w-md space-y-8 bg-base-100 dark:bg-base-200/90 backdrop-blur-xl p-8 rounded-3xl border border-base-200 dark:border-base-100/20 shadow-2xl">
                <div class="text-center space-y-2">
                    <div class="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-primary/10 text-primary text-3xl mb-2">
                        "🌴"
                    </div>
                    <span class="block text-xs font-bold uppercase tracking-[0.2em] text-primary">"Administrative Portal"</span>
                    <h2 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "Sign in to Our Places"
                    </h2>
                    <p class="text-xs text-base-content/60">
                        "Host and Administrator Management Console"
                    </p>
                </div>

                // Redirect Notice Container
                <div id="admin-login-notice" class="alert alert-info text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                    <span>"🔒"</span>
                    <span id="admin-login-notice-text">"Authentication required. Please sign in to continue."</span>
                </div>

                // Error Message Container
                <div id="admin-login-error" class="alert alert-error text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                    <span>"⚠️"</span>
                    <span id="admin-login-error-text">"Authentication error occurred"</span>
                </div>

                <form
                    class="mt-6 space-y-5"
                    id="admin-login-form"
                    action="javascript:void(0);"
                    onsubmit="if(window.handleAdminLogin){window.handleAdminLogin(event);} return false;"
                >
                    <div class="space-y-4">
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                                "Email address"
                            </label>
                            <input
                                type="email"
                                id="admin-login-email"
                                required=(true)
                                placeholder="user@email.com"
                                onkeydown="if(event.key==='Enter'){event.preventDefault();if(window.handleAdminLogin)window.handleAdminLogin(event);}"
                                class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                            />
                        </div>

                        <div>
                            <div class="flex justify-between items-center mb-1.5">
                                <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70">
                                    "Password"
                                </label>
                                <a href="#" tabindex="-1" class="text-xs text-primary hover:underline font-medium">
                                    "Forgot password?"
                                </a>
                            </div>
                            <input
                                type="password"
                                id="admin-login-password"
                                required=(true)
                                placeholder="••••••••"
                                onkeydown="if(event.key==='Enter'){event.preventDefault();if(window.handleAdminLogin)window.handleAdminLogin(event);}"
                                class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                            />
                        </div>
                    </div>

                    <div class="flex items-center justify-between">
                        <label class="flex items-center gap-2 cursor-pointer">
                            <input type="checkbox" checked=(true) class="checkbox checkbox-primary checkbox-sm rounded-md" />
                            <span class="text-xs text-base-content/70 font-medium">"Remember session"</span>
                        </label>
                        <span class="badge badge-warning badge-xs font-bold">"2FA Protected"</span>
                    </div>

                    <div>
                        <button
                            id="btn-admin-login"
                            type="button"
                            onclick="if(window.handleAdminLogin)window.handleAdminLogin(event)"
                            class="btn btn-primary w-full rounded-full py-3 shadow-lg tracking-wider uppercase text-xs font-bold flex items-center justify-center gap-2 cursor-pointer transition-all duration-200"
                        >
                            <span id="btn-admin-login-spinner" class="loading loading-spinner loading-xs hidden"></span>
                            <span id="btn-admin-login-text">"Sign In to Dashboard"</span>
                            <span id="btn-admin-login-arrow">"›"</span>
                        </button>
                    </div>
                </form>


                <div class="pt-4 border-t border-base-200/80 text-center">
                    <p class="text-xs text-base-content/50">
                        "Protected by Google Cloud IAM & Secure Admin Session Cookies"
                    </p>
                </div>
            </div>
        </div>

        <script>
            r#"
            (function() {
                var redirectTarget = '';
                try {
                    var params = new URLSearchParams(window.location.search);
                    redirectTarget = params.get('redirect') || '';
                    if (redirectTarget) {
                        var notice = document.getElementById('admin-login-notice');
                        var noticeText = document.getElementById('admin-login-notice-text');
                        if (notice) {
                            notice.style.setProperty('display', 'flex', 'important');
                        }
                    }
                } catch(e) {}

                function setButtonLoading(loading) {
                    var btn = document.getElementById('btn-admin-login');
                    var spinner = document.getElementById('btn-admin-login-spinner');
                    var text = document.getElementById('btn-admin-login-text');
                    var arrow = document.getElementById('btn-admin-login-arrow');
                    if (!btn) return;
                    if (loading) {
                        btn.disabled = true;
                        btn.classList.add('opacity-80', 'cursor-not-allowed');
                        if (spinner) spinner.classList.remove('hidden');
                        if (arrow) arrow.classList.add('hidden');
                        if (text) text.innerText = 'Verifying Credentials...';
                    } else {
                        btn.disabled = false;
                        btn.classList.remove('opacity-80', 'cursor-not-allowed');
                        if (spinner) spinner.classList.add('hidden');
                        if (arrow) arrow.classList.remove('hidden');
                        if (text) text.innerText = 'Sign In to Dashboard';
                    }
                }

                function showError(msg) {
                    var errBox = document.getElementById('admin-login-error');
                    var errText = document.getElementById('admin-login-error-text');
                    if (errBox) {
                        if (errText) {
                            errText.innerText = msg;
                            errBox.style.setProperty('display', 'flex', 'important');
                        }
                    }
                }

                function hideError() {
                    var errBox = document.getElementById('admin-login-error');
                    if (errBox) {
                        errBox.style.setProperty('display', 'none', 'important');
                    }
                }

                window.handleAdminLogin = function(e) {
                    if (e) {
                        try {
                            e.preventDefault();
                            e.stopPropagation();
                        } catch(err) {}
                    }
                    hideError();

                    var emailInput = document.getElementById('admin-login-email');
                    var passwordInput = document.getElementById('admin-login-password');
                    var email = '';
                    var password = '';

                    if (emailInput) {
                        if (emailInput.value) {
                            email = emailInput.value.trim();
                        }
                    }
                    if (passwordInput) {
                        if (passwordInput.value) {
                            password = passwordInput.value.trim();
                        }
                    }

                    if (!email) {
                        showError('Please enter your email address.');
                        return false;
                    }

                    if (email.indexOf('@') === -1) {
                        showError('Please enter a valid email address.');
                        return false;
                    }

                    if (!password) {
                        showError('Please enter your password.');
                        return false;
                    }

                    setButtonLoading(true);

                    // Post to server-side auth endpoint which sets cookie and persists session
                    fetch('/api/auth/login', {
                        method: 'POST',
                        credentials: 'include',
                        headers: {
                            'Content-Type': 'application/json',
                            'Accept': 'application/json'
                        },
                        body: JSON.stringify({ email: email, password: password })
                    })
                    .then(function(response) {
                        return response.json().then(function(payload) {
                            if (payload.success) {
                                if (payload.user) {
                                    var userObj = payload.user;
                                    var fullName = '';
                                    if (userObj.first_name) {
                                        fullName = userObj.first_name;
                                        if (userObj.last_name) {
                                            fullName = fullName + ' ' + userObj.last_name;
                                        }
                                    }
                                    if (!fullName) {
                                        fullName = userObj.email || 'Administrator';
                                    }

                                    var primaryRole = 'admin';
                                    if (userObj.roles) {
                                        if (userObj.roles.length) {
                                            primaryRole = userObj.roles[0];
                                        }
                                    }


                                    localStorage.setItem('op_auth_user', JSON.stringify({
                                        id: userObj.id,
                                        name: fullName,
                                        email: userObj.email || email,
                                        role: primaryRole
                                    }));

                                    var target = redirectTarget || '/admin';
                                    window.location.replace(target);
                                    return;
                                }
                            }
                            setButtonLoading(false);
                            showError(payload.message || 'Invalid email or password. Access restricted to authorized administrators.');
                        });
                    })
                    .catch(function(networkErr) {
                        console.error('Server auth call failed:', networkErr);
                        setButtonLoading(false);
                        showError('Unable to connect to authentication server. Please check your connection and try again.');
                    });

                    return false;
                };


                var loginForm = document.getElementById('admin-login-form');
                if (loginForm) {
                    loginForm.addEventListener('submit', function(e) {
                        if (e) {
                            e.preventDefault();
                            e.stopPropagation();
                        }
                        window.handleAdminLogin(e);
                        return false;
                    });
                }
            })();
            "#
        </script>
    }
}

