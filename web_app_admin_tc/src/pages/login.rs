use common::models::{
    CreateSessionRequest, LoginRequest, NewBookerProfile, NewHostProfile, NewUserRequest,
    ResendVerificationRequest, UserResponse, VerifyRequest,
};
use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, page, route},
    view::{View, view},
};
use validator::Validate;
use web_app_common_tc::{auth::token_hash_to_hex, get_api_client};

/// Response type for administrator and host login API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminLoginResponse {
    /// Whether authentication succeeded.
    pub success: bool,
    /// Authenticated user record.
    pub user: Option<UserResponse>,
    /// Error or informational message.
    pub message: Option<String>,
}

/// Request payload for host registration on the administrative portal.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct HostRegisterRequest {
    /// Host first name.
    #[validate(length(min = 1, message = "First name is required"))]
    pub first_name: String,
    /// Host last name.
    #[validate(length(min = 1, message = "Last name is required"))]
    pub last_name: String,
    /// Host email address.
    #[validate(email(message = "Invalid email address format"))]
    pub email: String,
    /// Account password (minimum 8 characters).
    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub password: String,
    /// Contact phone number.
    pub phone_number: Option<String>,
}

/// Response returned after host registration attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostRegisterResponse {
    /// Whether registration succeeded.
    pub success: bool,
    /// Registered email address.
    pub email: Option<String>,
    /// Informational or error message.
    pub message: Option<String>,
}

/// Response returned after email OTP verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostVerifyResponse {
    /// Whether email verification succeeded.
    pub success: bool,
    /// Verified user details.
    pub user: Option<UserResponse>,
    /// Informational or error message.
    pub message: Option<String>,
}

/// Authenticates administrator or host credentials, verifies portal authorization,
/// and establishes a server-side session.
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
                    message: Some(
                        "Access denied. Host or administrator role required.".to_string(),
                    ),
                }));
            }

            // Start Topcoat session (issues op_admin_session cookie to client)
            match topcoat::session::start(cx).await {
                Ok(session) => {
                    let hash_hex = token_hash_to_hex(&session.token_hash);
                    let primary_role = if user_resp.roles.iter().any(|r| {
                        let lr = r.to_lowercase();
                        lr == "admin" || lr == "superadmin"
                    }) {
                        "admin".to_string()
                    } else if user_resp.roles.iter().any(|r| r.to_lowercase() == "host") {
                        "host".to_string()
                    } else {
                        user_resp
                            .roles
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "host".to_string())
                    };
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
                            message: Some(
                                "Failed to persist session. Please try again.".to_string(),
                            ),
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
            let err_str = e.to_string();
            let msg = if err_str.contains("Account not verified")
                || err_str.contains("not verified")
            {
                "Account not verified. Please enter the verification code sent to your email."
                    .to_string()
            } else {
                "Invalid email or password. Access restricted to authorized hosts and administrators.".to_string()
            };
            Ok(Json(AdminLoginResponse {
                success: false,
                user: None,
                message: Some(msg),
            }))
        }
    }
}

/// Public host registration endpoint.
///
/// Strictly registers users with `host` and `booker` roles and default profiles.
/// Administrators CANNOT sign up through this endpoint; other admin accounts must
/// be created by existing administrators via `/api/admin/users/create`.
#[route(POST "/api/auth/register")]
pub async fn host_register_api(
    cx: &Cx,
    Json(payload): Json<HostRegisterRequest>,
) -> Result<Json<HostRegisterResponse>> {
    let api = get_api_client(cx);

    if let Err(e) = payload.validate() {
        return Ok(Json(HostRegisterResponse {
            success: false,
            email: None,
            message: Some(format!("Validation failed: {}", e)),
        }));
    }

    // Default host profile configuration
    let default_host_profile = NewHostProfile {
        verified_status: Some("pending".to_string()),
        payout_details: Some(serde_json::json!({})),
        description: Some("Host registered via Executive Console".to_string()),
    };

    // Default booker profile so the host can also book as a guest
    let default_booker_profile = NewBookerProfile {
        emergency_contacts: None,
        booking_preferences: None,
        loyalty: None,
    };

    // Capabilities: host can manage listings and bookings (admin capabilities forbidden)
    let mut attrs = serde_json::Map::new();
    attrs.insert(
        "can_manage_listings".to_string(),
        serde_json::Value::Bool(true),
    );
    attrs.insert(
        "can_manage_bookings".to_string(),
        serde_json::Value::Bool(true),
    );

    // Hardcoded roles: "host" and "booker". Admins cannot sign up!
    let new_user = NewUserRequest {
        email: payload.email.trim().to_lowercase(),
        password: payload.password,
        first_name: payload.first_name.trim().to_string(),
        last_name: payload.last_name.trim().to_string(),
        phone_number: payload
            .phone_number
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty()),
        is_active: true,
        is_verified: false,
        attributes: Some(serde_json::Value::Object(attrs)),
        roles: Some(vec!["host".to_string(), "booker".to_string()]),
        booker_profile: Some(default_booker_profile),
        host_profile: Some(default_host_profile),
        default_currency: Some("USD".to_string()),
    };

    match api.create_user(&new_user).await {
        Ok(_) => Ok(Json(HostRegisterResponse {
            success: true,
            email: Some(new_user.email),
            message: Some(
                "Host account created! Please check your email for the 6-digit verification code."
                    .to_string(),
            ),
        })),
        Err(err) => {
            tracing::warn!("Host registration failed: {:?}", err);
            let msg = err.to_string();
            let clean_msg = if msg.contains("Email already taken") || msg.contains("unique") {
                "An account with this email address already exists.".to_string()
            } else {
                "Registration failed. Please check your information and try again.".to_string()
            };
            Ok(Json(HostRegisterResponse {
                success: false,
                email: None,
                message: Some(clean_msg),
            }))
        }
    }
}

/// Verifies a host's email with the 6-digit OTP code and establishes an administrative portal session.
#[route(POST "/api/auth/verify")]
pub async fn host_verify_api(
    cx: &Cx,
    Json(payload): Json<VerifyRequest>,
) -> Result<Json<HostVerifyResponse>> {
    let api = get_api_client(cx);

    let verify_req = VerifyRequest {
        email: payload.email.trim().to_lowercase(),
        code: payload.code.trim().to_uppercase(),
    };

    match api.verify_user(&verify_req).await {
        Ok(user_resp) => {
            let is_authorized = user_resp.roles.iter().any(|r| {
                let lr = r.to_lowercase();
                lr == "admin" || lr == "superadmin" || lr == "host"
            });

            if !is_authorized {
                return Ok(Json(HostVerifyResponse {
                    success: false,
                    user: None,
                    message: Some(
                        "Access denied. Host or administrator role required.".to_string(),
                    ),
                }));
            }

            // Start Topcoat session so host is immediately logged in
            match topcoat::session::start(cx).await {
                Ok(session) => {
                    let hash_hex = token_hash_to_hex(&session.token_hash);
                    let full_name = format!("{} {}", user_resp.first_name, user_resp.last_name)
                        .trim()
                        .to_string();

                    let primary_role = if user_resp.roles.iter().any(|r| {
                        let lr = r.to_lowercase();
                        lr == "admin" || lr == "superadmin"
                    }) {
                        "admin".to_string()
                    } else {
                        "host".to_string()
                    };

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
                        ttl_seconds: 604800,
                    };

                    if let Err(e) = api.create_session(&create_req).await {
                        tracing::error!(
                            "Failed to persist admin session during host verification: {:?}",
                            e
                        );
                    }

                    Ok(Json(HostVerifyResponse {
                        success: true,
                        user: Some(user_resp),
                        message: None,
                    }))
                }
                Err(e) => {
                    tracing::error!("Failed to start session during host verification: {:?}", e);
                    Ok(Json(HostVerifyResponse {
                        success: true,
                        user: Some(user_resp),
                        message: None,
                    }))
                }
            }
        }
        Err(err) => {
            tracing::warn!("Host verification failed: {:?}", err);
            let msg = err.to_string();
            let clean_msg = if msg.contains("Invalid verification code") {
                "Invalid verification code. Please check your email and enter the 6-digit code."
                    .to_string()
            } else if msg.contains("expired") {
                "Verification code has expired. Please click 'Resend Code' to receive a new one."
                    .to_string()
            } else {
                "Verification failed. Please check the code and try again.".to_string()
            };
            Ok(Json(HostVerifyResponse {
                success: false,
                user: None,
                message: Some(clean_msg),
            }))
        }
    }
}

/// Resends the email verification OTP code to the requested host.
#[route(POST "/api/auth/resend-code")]
pub async fn host_resend_code_api(
    cx: &Cx,
    Json(payload): Json<ResendVerificationRequest>,
) -> Result<Json<AdminLoginResponse>> {
    let api = get_api_client(cx);
    let req = ResendVerificationRequest {
        email: payload.email.trim().to_lowercase(),
    };

    match api.resend_verification(&req).await {
        Ok(user_resp) => Ok(Json(AdminLoginResponse {
            success: true,
            user: Some(user_resp),
            message: Some("A new verification code has been sent to your email.".to_string()),
        })),
        Err(err) => {
            tracing::warn!("Failed to resend verification code: {:?}", err);
            Ok(Json(AdminLoginResponse {
                success: false,
                user: None,
                message: Some(
                    "Failed to resend code. Account may already be verified.".to_string(),
                ),
            }))
        }
    }
}

/// Signs out of the administrative console by revoking session cookies and cache.
#[page("/logout")]
pub async fn logout_page(cx: &Cx) -> Result<impl View> {
    if let Ok(Some(token_hash)) = topcoat::session::token_hash(cx).await {
        let hash_hex = token_hash_to_hex(&token_hash);
        let api = get_api_client(cx);
        let _ = api.delete_session(&hash_hex).await;
    }
    let _ = topcoat::session::stop(cx).await;

    Ok(view! {
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
    })
}

/// Administrative Portal Login & Host Signup Page.
/// Features dual tabs for administrator/host sign in and new host account creation.
#[page("/login")]
pub async fn login_page(cx: &Cx) -> Result<impl View> {
    let is_logged_in = web_app_common_tc::auth::get_admin_session(cx)
        .await
        .is_some();

    Ok(view! {
        if is_logged_in {
            <script>
                r#"
                var params = new URLSearchParams(window.location.search);
                var target = params.get('redirect') || '/admin';
                window.location.replace(target);
                "#
            </script>
        } else {
            <div class="min-h-[75vh] flex items-center justify-center py-12 px-4 sm:px-6 lg:px-8">
            <div class="w-full max-w-lg space-y-6 bg-base-100 dark:bg-base-200/90 backdrop-blur-xl p-8 rounded-3xl border border-base-200 dark:border-base-100/20 shadow-2xl">
                <div class="text-center space-y-2">
                    <div class="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-primary/10 text-primary text-3xl mb-1">
                        "🌴"
                    </div>
                    <span class="block text-xs font-bold uppercase tracking-[0.2em] text-primary">"Administrative & Host Portal"</span>
                    <h2 id="auth-card-title" class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "Sign in to Our Places"
                    </h2>
                    <p id="auth-card-subtitle" class="text-xs text-base-content/60">
                        "Executive management console for luxury Jamaican villas"
                    </p>
                </div>

                // Tabs Switcher: Sign In vs Host Sign Up
                <div class="tabs tabs-boxed bg-base-200 dark:bg-base-300/80 p-1 rounded-2xl grid grid-cols-2 gap-1">
                    <button
                        type="button"
                        id="tab-btn-signin"
                        onclick="window.switchAuthTab('signin')"
                        class="tab tab-active rounded-xl font-bold text-xs transition-all duration-200"
                    >
                        "Sign In"
                    </button>
                    <button
                        type="button"
                        id="tab-btn-signup"
                        onclick="window.switchAuthTab('signup')"
                        class="tab rounded-xl font-bold text-xs transition-all duration-200"
                    >
                        "Host Sign Up"
                    </button>
                </div>

                // Redirect Notice Container
                <div id="admin-login-notice" class="alert alert-info text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                    <span>"🔒"</span>
                    <span id="admin-login-notice-text">"Authentication required. Please sign in to continue."</span>
                </div>

                // Error Message Container
                <div id="admin-auth-error" class="alert alert-error text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                    <span>"⚠️"</span>
                    <span id="admin-auth-error-text">"Authentication error occurred"</span>
                </div>

                // Success Notice Container
                <div id="admin-auth-success" class="alert alert-success text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                    <span>"✅"</span>
                    <span id="admin-auth-success-text">"Action completed successfully"</span>
                </div>

                // TAB 1: SIGN IN FORM
                <div id="pane-signin">
                    <form
                        class="space-y-4"
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
                                    name="email"
                                    autocomplete="username"
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
                                    name="password"
                                    autocomplete="current-password"
                                    required=(true)
                                    placeholder="••••••••"
                                    onkeydown="if(event.key==='Enter'){event.preventDefault();if(window.handleAdminLogin)window.handleAdminLogin(event);}"
                                    class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                                />
                            </div>
                        </div>

                        <div class="flex items-center justify-between pt-1">
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

                    <div class="divider my-5 text-xs text-base-content/40 uppercase font-semibold">"New Host?"</div>

                    <div class="text-center text-xs">
                        <span class="text-base-content/70">"Want to list your luxury villa?"</span>
                        <button
                            type="button"
                            onclick="window.switchAuthTab('signup')"
                            class="link link-primary ml-1.5 font-bold hover:underline"
                        >
                            "Sign up as a Host"
                        </button>
                    </div>
                </div>

                // TAB 2: HOST SIGN UP FORM
                <div id="pane-signup" style="display: none;">
                    // Note banner explaining host capabilities and administrator creation restrictions
                    <div class="alert alert-info text-xs py-3 px-4 rounded-2xl font-medium shadow-xs items-start gap-2.5 mb-5">
                        <span class="text-base">"💡"</span>
                        <div class="space-y-0.5 text-left">
                            <div class="font-bold text-base-content">"Host Partner Account"</div>
                            <div class="text-base-content/80 text-[11px] leading-relaxed">
                                "Hosts can publish villa listings, manage bookings, and make reservations as a guest. Note: Other administrator accounts can only be created by existing administrators."
                            </div>
                        </div>
                    </div>

                    <form
                        class="space-y-4"
                        id="host-signup-form"
                        action="javascript:void(0);"
                        onsubmit="if(window.handleHostRegister){window.handleHostRegister(event);} return false;"
                    >
                        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                            <div>
                                <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                                    "First name"
                                </label>
                                <input
                                    type="text"
                                    id="host-reg-first-name"
                                    name="first_name"
                                    required=(true)
                                    placeholder="Jane"
                                    class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                                />
                            </div>
                            <div>
                                <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                                    "Last name"
                                </label>
                                <input
                                    type="text"
                                    id="host-reg-last-name"
                                    name="last_name"
                                    required=(true)
                                    placeholder="Doe"
                                    class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                                />
                            </div>
                        </div>

                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                                "Email address"
                            </label>
                            <input
                                type="email"
                                id="host-reg-email"
                                name="email"
                                autocomplete="email"
                                required=(true)
                                placeholder="host@example.com"
                                class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                            />
                        </div>

                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                                "Phone number (optional)"
                            </label>
                            <input
                                type="tel"
                                id="host-reg-phone"
                                name="phone_number"
                                placeholder="+1 (876) 555-0199"
                                class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                            />
                        </div>

                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                                "Password (min 8 characters)"
                            </label>
                            <input
                                type="password"
                                id="host-reg-password"
                                name="password"
                                autocomplete="new-password"
                                required=(true)
                                minlength="8"
                                placeholder="••••••••"
                                class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                            />
                        </div>

                        <div class="pt-2">
                            <button
                                id="btn-host-signup"
                                type="button"
                                onclick="if(window.handleHostRegister)window.handleHostRegister(event)"
                                class="btn btn-primary w-full rounded-full py-3 shadow-lg tracking-wider uppercase text-xs font-bold flex items-center justify-center gap-2 cursor-pointer transition-all duration-200"
                            >
                                <span id="btn-host-signup-spinner" class="loading loading-spinner loading-xs hidden"></span>
                                <span id="btn-host-signup-text">"Create Host Account"</span>
                                <span id="btn-host-signup-arrow">"›"</span>
                            </button>
                        </div>
                    </form>

                    <div class="divider my-5 text-xs text-base-content/40 uppercase font-semibold">"Existing User?"</div>

                    <div class="text-center text-xs">
                        <span class="text-base-content/70">"Already have a host or admin account?"</span>
                        <button
                            type="button"
                            onclick="window.switchAuthTab('signin')"
                            class="link link-primary ml-1.5 font-bold hover:underline"
                        >
                            "Sign In"
                        </button>
                    </div>
                </div>

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
                        if (notice) {
                            notice.style.setProperty('display', 'flex', 'important');
                        }
                    }

                    var tabParam = params.get('tab') || '';
                    if (tabParam === 'signup' || tabParam === 'register') {
                        window.switchAuthTab('signup');
                    }
                } catch(e) {}

                window.switchAuthTab = function(tab) {
                    var paneSignin = document.getElementById('pane-signin');
                    var paneSignup = document.getElementById('pane-signup');
                    var tabBtnSignin = document.getElementById('tab-btn-signin');
                    var tabBtnSignup = document.getElementById('tab-btn-signup');
                    var cardTitle = document.getElementById('auth-card-title');
                    var cardSubtitle = document.getElementById('auth-card-subtitle');

                    hideError();
                    hideSuccess();

                    if (tab === 'signup') {
                        if (paneSignin) paneSignin.style.display = 'none';
                        if (paneSignup) paneSignup.style.display = 'block';
                        if (tabBtnSignin) tabBtnSignin.classList.remove('tab-active');
                        if (tabBtnSignup) tabBtnSignup.classList.add('tab-active');
                        if (cardTitle) cardTitle.innerText = 'Join as a Villa Host';
                        if (cardSubtitle) cardSubtitle.innerText = 'List Jamaican luxury villas and manage reservations';
                    } else {
                        if (paneSignin) paneSignin.style.display = 'block';
                        if (paneSignup) paneSignup.style.display = 'none';
                        if (tabBtnSignin) tabBtnSignin.classList.add('tab-active');
                        if (tabBtnSignup) tabBtnSignup.classList.remove('tab-active');
                        if (cardTitle) cardTitle.innerText = 'Sign in to Our Places';
                        if (cardSubtitle) cardSubtitle.innerText = 'Executive management console for luxury Jamaican villas';
                    }
                };

                function setLoginButtonLoading(loading) {
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

                function setSignupButtonLoading(loading) {
                    var btn = document.getElementById('btn-host-signup');
                    var spinner = document.getElementById('btn-host-signup-spinner');
                    var text = document.getElementById('btn-host-signup-text');
                    var arrow = document.getElementById('btn-host-signup-arrow');
                    if (!btn) return;
                    if (loading) {
                        btn.disabled = true;
                        btn.classList.add('opacity-80', 'cursor-not-allowed');
                        if (spinner) spinner.classList.remove('hidden');
                        if (arrow) arrow.classList.add('hidden');
                        if (text) text.innerText = 'Creating Account...';
                    } else {
                        btn.disabled = false;
                        btn.classList.remove('opacity-80', 'cursor-not-allowed');
                        if (spinner) spinner.classList.add('hidden');
                        if (arrow) arrow.classList.remove('hidden');
                        if (text) text.innerText = 'Create Host Account';
                    }
                }

                function showError(msg, isHtml) {
                    var errBox = document.getElementById('admin-auth-error');
                    var errText = document.getElementById('admin-auth-error-text');
                    if (errBox) {
                        if (errText) {
                            if (isHtml) {
                                errText.innerHTML = msg;
                            } else {
                                errText.innerText = msg;
                            }
                            errBox.style.setProperty('display', 'flex', 'important');
                        }
                    }
                }

                function hideError() {
                    var errBox = document.getElementById('admin-auth-error');
                    if (errBox) {
                        errBox.style.setProperty('display', 'none', 'important');
                    }
                }

                function showSuccess(msg) {
                    var succBox = document.getElementById('admin-auth-success');
                    var succText = document.getElementById('admin-auth-success-text');
                    if (succBox) {
                        if (succText) {
                            succText.innerText = msg;
                            succBox.style.setProperty('display', 'flex', 'important');
                        }
                    }
                }

                function hideSuccess() {
                    var succBox = document.getElementById('admin-auth-success');
                    if (succBox) {
                        succBox.style.setProperty('display', 'none', 'important');
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
                    hideSuccess();

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

                    setLoginButtonLoading(true);

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
                                        fullName = userObj.email || 'Host';
                                    }

                                    var primaryRole = 'host';
                                    if (userObj.roles) {
                                        if (userObj.roles.length) {
                                            var hasAdmin = userObj.roles.some(function(r) {
                                                var lr = (r || '').toLowerCase();
                                                return lr === 'admin' || lr === 'superadmin';
                                            });
                                            if (hasAdmin) {
                                                primaryRole = 'admin';
                                            }
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
                            setLoginButtonLoading(false);
                            var msg = payload.message || 'Invalid email or password.';
                            if (msg.indexOf('not verified') !== -1) {
                                var errBox = document.getElementById('admin-auth-error');
                                var errText = document.getElementById('admin-auth-error-text');
                                if (errBox) {
                                    if (errText) {
                                        errText.innerText = 'Account not verified. Click here to enter your 6-digit verification code.';
                                        errText.onclick = function() {
                                            window.location.href = '/verify?email=' + encodeURIComponent(email);
                                        };
                                        errBox.style.setProperty('display', 'flex', 'important');
                                    }
                                }
                            } else {
                                showError(msg, false);
                            }
                        });
                    })
                    .catch(function(networkErr) {
                        console.error('Server auth call failed:', networkErr);
                        setLoginButtonLoading(false);
                        showError('Unable to connect to authentication server. Please check your connection and try again.', false);
                    });

                    return false;
                };

                window.handleHostRegister = function(e) {
                    if (e) {
                        try {
                            e.preventDefault();
                            e.stopPropagation();
                        } catch(err) {}
                    }
                    hideError();
                    hideSuccess();

                    var fnEl = document.getElementById('host-reg-first-name');
                    var lnEl = document.getElementById('host-reg-last-name');
                    var emailEl = document.getElementById('host-reg-email');
                    var phoneEl = document.getElementById('host-reg-phone');
                    var passEl = document.getElementById('host-reg-password');

                    var firstName = '';
                    if (fnEl) { if (fnEl.value) { firstName = fnEl.value.trim(); } }
                    var lastName = '';
                    if (lnEl) { if (lnEl.value) { lastName = lnEl.value.trim(); } }
                    var email = '';
                    if (emailEl) { if (emailEl.value) { email = emailEl.value.trim(); } }
                    var phone = '';
                    if (phoneEl) { if (phoneEl.value) { phone = phoneEl.value.trim(); } }
                    var password = '';
                    if (passEl) { if (passEl.value) { password = passEl.value.trim(); } }

                    if (!firstName) {
                        showError('Please enter your first name.', false);
                        return false;
                    }
                    if (!lastName) {
                        showError('Please enter your last name.', false);
                        return false;
                    }
                    if (!email || email.indexOf('@') === -1) {
                        showError('Please enter a valid email address.', false);
                        return false;
                    }
                    if (!password || [0, 1, 2, 3, 4, 5, 6, 7].indexOf(password.length) !== -1) {
                        showError('Password must be at least 8 characters.', false);
                        return false;
                    }

                    setSignupButtonLoading(true);

                    fetch('/api/auth/register', {
                        method: 'POST',
                        credentials: 'include',
                        headers: {
                            'Content-Type': 'application/json',
                            'Accept': 'application/json'
                        },
                        body: JSON.stringify({
                            first_name: firstName,
                            last_name: lastName,
                            email: email,
                            password: password,
                            phone_number: phone ? phone : null
                        })
                    })
                    .then(function(response) {
                        return response.json().then(function(payload) {
                            if (payload.success) {
                                showSuccess('Host account created! Redirecting to email verification...');
                                setTimeout(function() {
                                    window.location.href = '/verify?email=' + encodeURIComponent(email);
                                }, 600);
                                return;
                            }
                            setSignupButtonLoading(false);
                            showError(payload.message || 'Registration failed. Please check your information and try again.', false);
                        });
                    })
                    .catch(function(err) {
                        console.error('Host registration failed:', err);
                        setSignupButtonLoading(false);
                        showError('Unable to connect to registration server. Please try again.', false);
                    });

                    return false;
                };

                var loginForm = document.getElementById('admin-login-form');
                if (loginForm) {
                    loginForm.addEventListener('submit', function(e) {
                        if (e) { e.preventDefault(); e.stopPropagation(); }
                        window.handleAdminLogin(e);
                        return false;
                    });
                }

                var signupForm = document.getElementById('host-signup-form');
                if (signupForm) {
                    signupForm.addEventListener('submit', function(e) {
                        if (e) { e.preventDefault(); e.stopPropagation(); }
                        window.handleHostRegister(e);
                        return false;
                    });
                }
            })();
            "#
        </script>
        }
    })
}

/// Standalone Host Registration Page (`/register`).
#[page("/register")]
pub async fn register_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <div class="min-h-[75vh] flex items-center justify-center py-12 px-4 sm:px-6 lg:px-8">
            <div class="w-full max-w-lg space-y-6 bg-base-100 dark:bg-base-200/90 backdrop-blur-xl p-8 rounded-3xl border border-base-200 dark:border-base-100/20 shadow-2xl">
                <div class="text-center space-y-2">
                    <div class="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-primary/10 text-primary text-3xl mb-1">
                        "🌴"
                    </div>
                    <span class="block text-xs font-bold uppercase tracking-[0.2em] text-primary">"Partner With Our Places"</span>
                    <h2 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "Join as a Villa Host"
                    </h2>
                    <p class="text-xs text-base-content/60">
                        "Create your host account to publish listings, manage reservations, and earn rental income."
                    </p>
                </div>

                // Note banner
                <div class="alert alert-info text-xs py-3 px-4 rounded-2xl font-medium shadow-xs items-start gap-2.5">
                    <span class="text-base">"💡"</span>
                    <div class="space-y-0.5 text-left">
                        <div class="font-bold text-base-content">"Host Partner Account"</div>
                        <div class="text-base-content/80 text-[11px] leading-relaxed">
                            "Hosts can publish villa listings, manage bookings, and make reservations as a guest. Note: Other administrator accounts can only be created by existing administrators."
                        </div>
                    </div>
                </div>

                // Error Container
                <div id="reg-error" class="alert alert-error text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                    <span>"⚠️"</span>
                    <span id="reg-error-text">"Error occurred"</span>
                </div>

                // Success Container
                <div id="reg-success" class="alert alert-success text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                    <span>"✅"</span>
                    <span id="reg-success-text">"Success"</span>
                </div>

                <form
                    class="space-y-4"
                    id="page-host-signup-form"
                    action="javascript:void(0);"
                >
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                                "First name"
                            </label>
                            <input
                                type="text"
                                id="page-host-fn"
                                name="first_name"
                                required=(true)
                                placeholder="Jane"
                                class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                            />
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                                "Last name"
                            </label>
                            <input
                                type="text"
                                id="page-host-ln"
                                name="last_name"
                                required=(true)
                                placeholder="Doe"
                                class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                            />
                        </div>
                    </div>

                    <div>
                        <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                            "Email address"
                        </label>
                        <input
                            type="email"
                            id="page-host-email"
                            name="email"
                            autocomplete="email"
                            required=(true)
                            placeholder="host@example.com"
                            class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                        />
                    </div>

                    <div>
                        <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                            "Phone number (optional)"
                        </label>
                        <input
                            type="tel"
                            id="page-host-phone"
                            name="phone_number"
                            placeholder="+1 (876) 555-0199"
                            class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                        />
                    </div>

                    <div>
                        <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                            "Password (min 8 characters)"
                        </label>
                        <input
                            type="password"
                            id="page-host-pass"
                            name="password"
                            autocomplete="new-password"
                            required=(true)
                            minlength="8"
                            placeholder="••••••••"
                            class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                        />
                    </div>

                    <div class="pt-2">
                        <button
                            id="btn-page-signup"
                            type="submit"
                            class="btn btn-primary w-full rounded-full py-3 shadow-lg tracking-wider uppercase text-xs font-bold flex items-center justify-center gap-2 cursor-pointer transition-all duration-200"
                        >
                            <span id="btn-page-signup-spinner" class="loading loading-spinner loading-xs hidden"></span>
                            <span id="btn-page-signup-text">"Create Host Account"</span>
                            <span>"›"</span>
                        </button>
                    </div>
                </form>

                <div class="divider my-5 text-xs text-base-content/40 uppercase font-semibold">"Existing User?"</div>

                <div class="text-center text-xs">
                    <span class="text-base-content/70">"Already have an account?"</span>
                    <a href="/login" class="link link-primary ml-1.5 font-bold hover:underline">
                        "Sign In"
                    </a>
                </div>
            </div>
        </div>

        <script>
            r#"
            (function() {
                var form = document.getElementById('page-host-signup-form');
                if (!form) return;

                var errBox = document.getElementById('reg-error');
                var errText = document.getElementById('reg-error-text');
                var succBox = document.getElementById('reg-success');
                var succText = document.getElementById('reg-success-text');
                var btn = document.getElementById('btn-page-signup');
                var spinner = document.getElementById('btn-page-signup-spinner');
                var btnText = document.getElementById('btn-page-signup-text');

                form.onsubmit = function(e) {
                    if (e) { e.preventDefault(); e.stopPropagation(); }
                    if (errBox) errBox.style.display = 'none';
                    if (succBox) succBox.style.display = 'none';

                    var fn = document.getElementById('page-host-fn').value.trim();
                    var ln = document.getElementById('page-host-ln').value.trim();
                    var email = document.getElementById('page-host-email').value.trim();
                    var phone = document.getElementById('page-host-phone').value.trim();
                    var pass = document.getElementById('page-host-pass').value.trim();

                    if (!fn || !ln || !email || !pass) {
                        if (errText) errText.innerText = 'Please complete all required fields.';
                        if (errBox) errBox.style.display = 'flex';
                        return false;
                    }

                    if ([0, 1, 2, 3, 4, 5, 6, 7].indexOf(pass.length) !== -1) {
                        if (errText) errText.innerText = 'Password must be at least 8 characters.';
                        if (errBox) errBox.style.display = 'flex';
                        return false;
                    }

                    if (btn) btn.disabled = true;
                    if (spinner) spinner.classList.remove('hidden');
                    if (btnText) btnText.innerText = 'Creating Host Account...';

                    fetch('/api/auth/register', {
                        method: 'POST',
                        credentials: 'include',
                        headers: {
                            'Content-Type': 'application/json',
                            'Accept': 'application/json'
                        },
                        body: JSON.stringify({
                            first_name: fn,
                            last_name: ln,
                            email: email,
                            password: pass,
                            phone_number: phone ? phone : null
                        })
                    })
                    .then(function(res) {
                        return res.json().then(function(payload) {
                            if (payload.success) {
                                if (succText) succText.innerText = 'Host account created! Redirecting to email verification...';
                                if (succBox) succBox.style.display = 'flex';
                                setTimeout(function() {
                                    window.location.href = '/verify?email=' + encodeURIComponent(email);
                                }, 600);
                            } else {
                                if (btn) btn.disabled = false;
                                if (spinner) spinner.classList.add('hidden');
                                if (btnText) btnText.innerText = 'Create Host Account';
                                if (errText) errText.innerText = payload.message || 'Registration failed.';
                                if (errBox) errBox.style.display = 'flex';
                            }
                        });
                    })
                    .catch(function(err) {
                        console.error('Registration failed:', err);
                        if (btn) btn.disabled = false;
                        if (spinner) spinner.classList.add('hidden');
                        if (btnText) btnText.innerText = 'Create Host Account';
                        if (errText) errText.innerText = 'Unable to connect to registration server.';
                        if (errBox) errBox.style.display = 'flex';
                    });

                    return false;
                };
            })();
            "#
        </script>
    })
}

/// Email Verification Page (`/verify`) for Host Account Activation.
#[page("/verify")]
pub async fn verify_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <div class="min-h-[75vh] flex items-center justify-center py-12 px-4 sm:px-6 lg:px-8">
            <div class="w-full max-w-md space-y-6 bg-base-100 dark:bg-base-200/90 backdrop-blur-xl p-8 rounded-3xl border border-base-200 dark:border-base-100/20 shadow-2xl">
                <div class="text-center space-y-2">
                    <div class="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-primary/10 text-primary text-3xl mb-1">
                        "✉️"
                    </div>
                    <span class="block text-xs font-bold uppercase tracking-[0.2em] text-primary">"Account Activation"</span>
                    <h2 class="text-3xl font-serif font-bold tracking-tight text-base-content">
                        "Verify Email"
                    </h2>
                    <p class="text-xs text-base-content/60">
                        "Enter the 6-digit verification code sent to your email to activate host console access."
                    </p>
                </div>

                <div id="verify-info" class="alert alert-info text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                    <span>"ℹ️"</span>
                    <span id="verify-info-text">"A verification code has been sent to your email."</span>
                </div>

                <div id="verify-error" class="alert alert-error text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                    <span>"⚠️"</span>
                    <span id="verify-error-text">"Verification error"</span>
                </div>

                <form
                    id="host-verify-form"
                    class="space-y-4"
                    action="javascript:void(0);"
                >
                    <div>
                        <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5">
                            "Email Address"
                        </label>
                        <input
                            type="email"
                            id="verify-email"
                            name="email"
                            placeholder="host@example.com"
                            class="input input-bordered w-full rounded-xl bg-base-100 dark:bg-base-300 font-medium transition-all focus:border-primary"
                            required=(true)
                        />
                    </div>

                    <div>
                        <label class="block text-xs font-bold uppercase tracking-wider text-base-content/70 mb-1.5 text-center">
                            "6-Digit Verification Code"
                        </label>
                        <input
                            type="text"
                            id="verify-code"
                            name="code"
                            placeholder="123456"
                            maxlength="6"
                            class="input input-bordered w-full text-center text-2xl tracking-[0.5em] font-mono font-bold rounded-xl bg-base-100 dark:bg-base-300 focus:border-primary"
                            required=(true)
                        />
                    </div>

                    <div class="pt-2">
                        <button
                            id="btn-verify"
                            type="submit"
                            class="btn btn-primary w-full rounded-full py-3 shadow-lg tracking-wider uppercase text-xs font-bold flex items-center justify-center gap-2 cursor-pointer transition-all duration-200"
                        >
                            <span id="btn-verify-spinner" class="loading loading-spinner loading-xs hidden"></span>
                            <span id="btn-verify-text">"Verify & Launch Console"</span>
                            <span>"›"</span>
                        </button>
                    </div>
                </form>

                <div class="text-center text-xs text-base-content/70 pt-2 flex items-center justify-center gap-2">
                    <span>"Didn't receive code?"</span>
                    <button type="button" id="btn-resend-code" class="link link-primary font-bold hover:underline">
                        "Resend Code"
                    </button>
                </div>

                <div class="pt-4 border-t border-base-200/80 text-center">
                    <a href="/login" class="text-xs text-primary font-semibold hover:underline">
                        "← Back to Sign In"
                    </a>
                </div>
            </div>
        </div>

        <script>
            r#"
            (function() {
                var emailEl = document.getElementById('verify-email');
                var codeEl = document.getElementById('verify-code');
                var infoBox = document.getElementById('verify-info');
                var infoText = document.getElementById('verify-info-text');
                var errBox = document.getElementById('verify-error');
                var errText = document.getElementById('verify-error-text');

                try {
                    var params = new URLSearchParams(window.location.search);
                    var emailParam = params.get('email');
                    if (emailParam) {
                        var cleanEmail = emailParam.trim();
                        if (emailEl) emailEl.value = cleanEmail;
                        if (infoText) {
                            infoText.innerText = 'We sent a 6-digit verification code to ' + cleanEmail + '. Please enter it below to activate your account.';
                        }
                        if (infoBox) infoBox.style.setProperty('display', 'flex', 'important');
                        if (codeEl) codeEl.focus();
                    }
                } catch(e) {}

                var resendBtn = document.getElementById('btn-resend-code');
                if (resendBtn) {
                    resendBtn.onclick = function(e) {
                        if (e) { try { e.preventDefault(); } catch(err) {} }
                        if (errBox) errBox.style.setProperty('display', 'none', 'important');

                        var email = '';
                        if (emailEl) {
                            if (emailEl.value) {
                                email = emailEl.value.trim();
                            }
                        }
                        if (!email) {
                            if (errText) errText.innerText = 'Please enter your email address to resend the code.';
                            if (errBox) errBox.style.setProperty('display', 'flex', 'important');
                            return;
                        }

                        resendBtn.disabled = true;
                        resendBtn.innerText = 'Sending...';

                        fetch('/api/auth/resend-code', {
                            method: 'POST',
                            credentials: 'include',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ email: email })
                        })
                        .then(function(res) {
                            resendBtn.disabled = false;
                            resendBtn.innerText = 'Resend Code';
                            return res.json().then(function(payload) {
                                if (payload.success) {
                                    if (infoText) infoText.innerText = 'A new verification code has been sent to ' + email + '.';
                                    if (infoBox) infoBox.style.setProperty('display', 'flex', 'important');
                                } else {
                                    if (errText) errText.innerText = payload.message || 'Failed to resend code.';
                                    if (errBox) errBox.style.setProperty('display', 'flex', 'important');
                                }
                            });
                        })
                        .catch(function(err) {
                            resendBtn.disabled = false;
                            resendBtn.innerText = 'Resend Code';
                            if (errText) errText.innerText = 'Network error while requesting verification code.';
                            if (errBox) errBox.style.setProperty('display', 'flex', 'important');
                        });
                    };
                }

                var form = document.getElementById('host-verify-form');
                if (!form) return;

                form.onsubmit = function(e) {
                    if (e) {
                        try { e.preventDefault(); e.stopPropagation(); } catch(err) {}
                    }
                    if (errBox) errBox.style.setProperty('display', 'none', 'important');

                    var btn = document.getElementById('btn-verify');
                    var spinner = document.getElementById('btn-verify-spinner');
                    var btnText = document.getElementById('btn-verify-text');

                    var email = '';
                    if (emailEl) {
                        if (emailEl.value) {
                            email = emailEl.value.trim();
                        }
                    }
                    var code = '';
                    if (codeEl) {
                        if (codeEl.value) {
                            code = codeEl.value.trim();
                        }
                    }

                    if (!email) {
                        if (errText) errText.innerText = 'Please enter your email address.';
                        if (errBox) errBox.style.setProperty('display', 'flex', 'important');
                        return false;
                    }
                    if (!code) {
                        if (errText) errText.innerText = 'Please enter your 6-digit verification code.';
                        if (errBox) errBox.style.setProperty('display', 'flex', 'important');
                        return false;
                    }

                    if (btn) btn.disabled = true;
                    if (spinner) spinner.classList.remove('hidden');
                    if (btnText) btnText.innerText = 'Verifying Code...';

                    fetch('/api/auth/verify', {
                        method: 'POST',
                        credentials: 'include',
                        headers: {
                            'Content-Type': 'application/json',
                            'Accept': 'application/json'
                        },
                        body: JSON.stringify({ email: email, code: code })
                    })
                    .then(function(res) {
                        return res.json().then(function(payload) {
                            if (payload.success) {
                                var u = payload.user || {};
                                var fullName = '';
                                if (u.first_name) {
                                    fullName = u.first_name;
                                    if (u.last_name) fullName = fullName + ' ' + u.last_name;
                                }
                                if (!fullName) fullName = u.email || 'Host Partner';

                                localStorage.setItem('op_auth_user', JSON.stringify({
                                    id: u.id,
                                    name: fullName,
                                    email: u.email || email,
                                    role: 'host'
                                }));

                                if (btnText) btnText.innerText = 'Verified! Launching Console...';
                                setTimeout(function() {
                                    window.location.replace('/admin');
                                }, 500);
                            } else {
                                if (btn) btn.disabled = false;
                                if (spinner) spinner.classList.add('hidden');
                                if (btnText) btnText.innerText = 'Verify and Launch Console';
                                if (errText) errText.innerText = payload.message || 'Invalid verification code.';
                                if (errBox) errBox.style.setProperty('display', 'flex', 'important');
                            }
                        });
                    })
                    .catch(function(err) {
                        console.error('Host verify connection failed:', err);
                        if (btn) btn.disabled = false;
                        if (spinner) spinner.classList.add('hidden');
                        if (btnText) btnText.innerText = 'Verify and Launch Console';
                        if (errText) errText.innerText = 'Unable to connect to verification server.';
                        if (errBox) errBox.style.setProperty('display', 'flex', 'important');
                    });

                    return false;
                };
            })();
            "#
        </script>
    })
}
