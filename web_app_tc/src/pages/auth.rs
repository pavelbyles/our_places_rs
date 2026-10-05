use common::models::{CreateSessionRequest, LoginRequest, UserResponse};
use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, page, route},
    view::{View, view},
};
use web_app_common_tc::{auth::token_hash_to_hex, get_api_client};

/// Response type for guest login API.
#[derive(Serialize, Deserialize)]
pub struct GuestLoginResponse {
    pub success: bool,
    pub user: Option<UserResponse>,
    pub message: Option<String>,
}

/// Server-side guest login: authenticates via user_api, starts a topcoat session,
/// and persists the session with namespace "guest" so `get_guest_session` can find it.
#[route(POST "/api/auth/login")]
pub async fn guest_login_api(
    cx: &Cx,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<GuestLoginResponse>> {
    let api = get_api_client(cx);

    match api.login_user(&payload).await {
        Ok(user_resp) => {
            // Start topcoat session (issues op_guest_session cookie to client)
            match topcoat::session::start(cx).await {
                Ok(session) => {
                    let hash_hex = token_hash_to_hex(&session.token_hash);
                    let full_name = format!("{} {}", user_resp.first_name, user_resp.last_name)
                        .trim()
                        .to_string();

                    // Persist session into user_api with namespace: "guest"
                    let create_req = CreateSessionRequest {
                        token_hash: hash_hex,
                        user_id: user_resp.id,
                        email: user_resp.email.clone(),
                        name: if full_name.is_empty() {
                            user_resp.email.clone()
                        } else {
                            full_name
                        },
                        role: "guest".to_string(),
                        namespace: "guest".to_string(),
                        ttl_seconds: 604800, // 7-day TTL with rolling refresh
                    };

                    if let Err(e) = api.create_session(&create_req).await {
                        tracing::error!("Failed to persist guest session in user_api: {:?}", e);
                        return Ok(Json(GuestLoginResponse {
                            success: false,
                            user: None,
                            message: Some(
                                "Failed to persist session. Please try again.".to_string(),
                            ),
                        }));
                    }

                    Ok(Json(GuestLoginResponse {
                        success: true,
                        user: Some(user_resp),
                        message: None,
                    }))
                }
                Err(e) => {
                    tracing::error!("Failed to start topcoat guest session: {:?}", e);
                    Ok(Json(GuestLoginResponse {
                        success: false,
                        user: None,
                        message: Some("Failed to initialize server session".to_string()),
                    }))
                }
            }
        }
        Err(e) => {
            tracing::warn!("Guest login failed for email {}: {:?}", payload.email, e);
            Ok(Json(GuestLoginResponse {
                success: false,
                user: None,
                message: Some("Invalid email or password.".to_string()),
            }))
        }
    }
}

#[page("/logout")]
pub async fn logout_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <div class="min-h-[60vh] flex items-center justify-center py-10 px-4">
            <div class="text-center space-y-4">
                <span class="loading loading-spinner loading-lg text-primary"></span>
                <p class="text-sm font-medium text-base-content/70">"Signing out..."</p>
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

#[page("/login")]
pub async fn login_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <div class="min-h-[70vh] flex items-center justify-center py-10 px-4">
            <div class="card w-full max-w-md bg-base-100 shadow-xl border border-base-200 overflow-hidden rounded-3xl">
                <div class="bg-primary text-primary-content p-8 text-center">
                    <h2 class="text-3xl font-extrabold">"Welcome Back"</h2>
                    <p class="mt-2 text-sm opacity-90">"Log in to manage your bookings and saved stays"</p>
                </div>

                <div class="p-8 space-y-6">
                    <div id="guest-login-error" class="alert alert-error text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                        <span>"⚠️"</span>
                        <span id="guest-login-error-text">"Authentication error"</span>
                    </div>

                    <form
                        id="guest-login-form"
                        class="space-y-4"
                    >
                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Email Address"</label>
                            <input type="email" id="guest-email" name="email" placeholder="you@example.com" class="input input-bordered w-full" required=(true) />
                        </div>

                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Password"</label>
                            <input type="password" id="guest-password" name="password" placeholder="••••••••" class="input input-bordered w-full" required=(true) />
                        </div>

                        <button id="btn-guest-login" type="submit" class="btn btn-primary w-full mt-4 font-bold flex items-center justify-center gap-2">
                            <span id="btn-guest-spinner" class="loading loading-spinner loading-xs hidden"></span>
                            <span id="btn-guest-text">"Log In"</span>
                        </button>
                    </form>

                    <div class="divider my-4">"OR"</div>

                    <a href="/verify" class="btn btn-outline w-full">
                        "Log In with Email Code (Passwordless)"
                    </a>

                    <div class="text-center text-sm pt-4">
                        <span class="text-base-content/70">"Don't have an account?"</span>
                        <a href="/register" class="link link-primary ml-2 font-semibold">"Sign Up"</a>
                    </div>
                </div>
            </div>
        </div>

        <script>
            r#"
            (function() {
                var form = document.getElementById('guest-login-form');
                if (!form) return;
                form.onsubmit = function(e) {
                    if (e) {
                        try {
                            e.preventDefault();
                            e.stopPropagation();
                        } catch(err) {}
                    }
                
                var errBox = document.getElementById('guest-login-error');
                var errText = document.getElementById('guest-login-error-text');
                if (errBox) {
                    errBox.style.display = 'none';
                }

                var emailEl = document.getElementById('guest-email');
                var passEl = document.getElementById('guest-password');
                var btn = document.getElementById('btn-guest-login');
                var spinner = document.getElementById('btn-guest-spinner');
                var btnText = document.getElementById('btn-guest-text');

                var email = '';
                var pass = '';
                if (emailEl) {
                    if (emailEl.value) {
                        email = emailEl.value.trim();
                    }
                }
                if (passEl) {
                    if (passEl.value) {
                        pass = passEl.value.trim();
                    }
                }

                if (!email) {
                    if (errText) errText.innerText = 'Please enter your email.';
                    if (errBox) errBox.style.display = 'flex';
                    return false;
                }
                if (!pass) {
                    if (errText) errText.innerText = 'Please enter your password.';
                    if (errBox) errBox.style.display = 'flex';
                    return false;
                }

                if (btn) btn.disabled = true;
                if (spinner) spinner.classList.remove('hidden');
                if (btnText) btnText.innerText = 'Authenticating...';

                fetch('/api/auth/login', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Accept': 'application/json'
                    },
                    body: JSON.stringify({ email: email, password: pass })
                })
                .then(function(res) {
                    return res.json().then(function(data) {
                        if (data.success) {
                            var u = data.user || {};
                            var fn = u.first_name || '';
                            var ln = u.last_name || '';
                            var name = (fn + ' ' + ln).trim();
                            if (!name) {
                                name = u.name || 'Guest User';
                            }
                            var phone = u.phone_number || u.phone || '';
                            var uid = u.id || '';
                            if (window.loginUser) {
                                window.loginUser(name, u.email || email, 'guest', fn, ln, phone, uid);
                            } else {
                                localStorage.setItem('op_auth_user', JSON.stringify({
                                    name: name,
                                    email: u.email || email,
                                    role: 'guest',
                                    first_name: fn,
                                    last_name: ln,
                                    phone: phone,
                                    id: uid
                                }));
                                window.location.href = '/';
                            }
                        } else {
                            if (btn) btn.disabled = false;
                            if (spinner) spinner.classList.add('hidden');
                            if (btnText) btnText.innerText = 'Log In';
                            var msg = data.message || 'Invalid email or password.';
                            if (msg.indexOf('not verified') !== -1) {
                                if (errText) {
                                    errText.innerHTML = 'Account not verified. <a href="/verify?email=' + encodeURIComponent(email) + '" class="underline font-bold ml-1">Verify now &rarr;</a>';
                                }
                            } else {
                                if (errText) errText.innerText = msg;
                            }
                            if (errBox) errBox.style.display = 'flex';
                        }
                    });
                })
                .catch(function(err) {
                    console.error('Login request failed:', err);
                    if (btn) btn.disabled = false;
                    if (spinner) spinner.classList.add('hidden');
                    if (btnText) btnText.innerText = 'Log In';
                    if (errText) errText.innerText = 'Unable to connect to authentication service. Please try again.';
                    if (errBox) errBox.style.display = 'flex';
                });

                return false;
                };
            })();
            "#
        </script>
    })
}

#[page("/register")]
pub async fn register_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <div class="min-h-[70vh] flex items-center justify-center py-10 px-4">
            <div class="card w-full max-w-md bg-base-100 shadow-xl border border-base-200 overflow-hidden rounded-3xl">
                <div class="bg-primary text-primary-content p-8 text-center">
                    <h2 class="text-3xl font-extrabold">"Create Account"</h2>
                    <p class="mt-2 text-sm opacity-90">"Join Our Places to unlock exclusive villa rates"</p>
                </div>

                <div class="p-8 space-y-6">
                    <div id="guest-reg-error" class="alert alert-error text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                        <span>"⚠️"</span>
                        <span id="guest-reg-error-text">"Registration error"</span>
                    </div>

                    <form
                        id="guest-reg-form"
                        class="space-y-4"
                    >
                        <div class="grid grid-cols-2 gap-4">
                            <div>
                                <label class="label text-xs font-bold uppercase text-base-content/70">"First Name"</label>
                                <input type="text" id="reg-first-name" name="first_name" placeholder="John" class="input input-bordered w-full" required=(true) />
                            </div>
                            <div>
                                <label class="label text-xs font-bold uppercase text-base-content/70">"Last Name"</label>
                                <input type="text" id="reg-last-name" name="last_name" placeholder="Doe" class="input input-bordered w-full" required=(true) />
                            </div>
                        </div>

                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Email Address"</label>
                            <input type="email" id="reg-email" name="email" placeholder="you@example.com" class="input input-bordered w-full" required=(true) />
                        </div>

                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Password"</label>
                            <input type="password" id="reg-password" name="password" placeholder="••••••••" class="input input-bordered w-full" required=(true) />
                        </div>

                        <button id="btn-guest-reg" type="submit" class="btn btn-primary w-full mt-4 font-bold flex items-center justify-center gap-2">
                            <span id="btn-reg-spinner" class="loading loading-spinner loading-xs hidden"></span>
                            <span id="btn-reg-text">"Create Account"</span>
                        </button>
                    </form>

                    <div class="text-center text-sm pt-4">
                        <span class="text-base-content/70">"Already have an account?"</span>
                        <a href="/login" class="link link-primary ml-2 font-semibold">"Log In"</a>
                    </div>
                </div>
            </div>
        </div>

        <script>
            r#"
            (function() {
                var form = document.getElementById('guest-reg-form');
                if (!form) return;
                form.onsubmit = function(e) {
                    if (e) {
                        try {
                            e.preventDefault();
                            e.stopPropagation();
                        } catch(err) {}
                    }

                var errBox = document.getElementById('guest-reg-error');
                var errText = document.getElementById('guest-reg-error-text');
                if (errBox) errBox.style.display = 'none';

                var fnEl = document.getElementById('reg-first-name');
                var lnEl = document.getElementById('reg-last-name');
                var emailEl = document.getElementById('reg-email');
                var passEl = document.getElementById('reg-password');
                var btn = document.getElementById('btn-guest-reg');
                var spinner = document.getElementById('btn-reg-spinner');
                var btnText = document.getElementById('btn-reg-text');

                var firstName = '';
                if (fnEl) { if (fnEl.value) firstName = fnEl.value.trim(); }
                var lastName = '';
                if (lnEl) { if (lnEl.value) lastName = lnEl.value.trim(); }
                var email = '';
                if (emailEl) { if (emailEl.value) email = emailEl.value.trim(); }
                var pass = '';
                if (passEl) { if (passEl.value) pass = passEl.value.trim(); }

                if (!email) {
                    if (errText) errText.innerText = 'Please enter your email.';
                    if (errBox) errBox.style.display = 'flex';
                    return false;
                }

                if (btn) btn.disabled = true;
                if (spinner) spinner.classList.remove('hidden');
                if (btnText) btnText.innerText = 'Creating Account...';

                fetch('http://localhost:8083/api/v1/users', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Accept': 'application/json'
                    },
                    body: JSON.stringify({
                        first_name: firstName,
                        last_name: lastName,
                        email: email,
                        password: pass,
                        roles: ['booker'],
                        booker_profile: {}
                    })
                })
                .then(function(res) {
                    if (res.ok) {
                        if (btnText) btnText.innerText = 'Account created! Redirecting to verification...';
                        setTimeout(function() {
                            window.location.href = '/verify?email=' + encodeURIComponent(email);
                        }, 400);
                    } else {
                        return res.json().then(function(err) {
                            if (btn) btn.disabled = false;
                            if (spinner) spinner.classList.add('hidden');
                            if (btnText) btnText.innerText = 'Create Account';
                            var msg = 'Registration failed.';
                            if (err) {
                                if (err.message) {
                                    msg = err.message;
                                }
                            }
                            if (errText) errText.innerText = msg;
                            if (errBox) errBox.style.display = 'flex';
                        }).catch(function() {
                            if (btn) btn.disabled = false;
                            if (spinner) spinner.classList.add('hidden');
                            if (btnText) btnText.innerText = 'Create Account';
                            if (errText) errText.innerText = 'Registration failed. Please try again.';
                            if (errBox) errBox.style.display = 'flex';
                        });
                    }
                })
                .catch(function(err) {
                    console.error('user_api connection failed:', err);
                    if (btn) btn.disabled = false;
                    if (spinner) spinner.classList.add('hidden');
                    if (btnText) btnText.innerText = 'Create Account';
                    if (errText) errText.innerText = 'Unable to connect to user service. Please ensure user_api is running.';
                    if (errBox) errBox.style.display = 'flex';
                });

                return false;
                };
            })();
            "#
        </script>
    })
}

#[page("/verify")]
pub async fn verify_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <div class="min-h-[70vh] flex items-center justify-center py-10 px-4">
            <div class="card w-full max-w-md bg-base-100 shadow-xl border border-base-200 overflow-hidden rounded-3xl">
                <div class="bg-primary text-primary-content p-8 text-center">
                    <h2 class="text-3xl font-extrabold">"Verify Email"</h2>
                    <p class="mt-2 text-sm opacity-90">"Enter the 6-digit verification code sent to your email"</p>
                </div>

                <div class="p-8 space-y-6">
                    <div id="verify-info" class="alert alert-info text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                        <span>"ℹ️"</span>
                        <span id="verify-info-text">"A verification code has been sent to your email."</span>
                    </div>

                    <div id="verify-error" class="alert alert-error text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                        <span>"⚠️"</span>
                        <span id="verify-error-text">"Verification error"</span>
                    </div>

                    <form
                        id="guest-verify-form"
                        class="space-y-4"
                    >
                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Email Address"</label>
                            <input
                                type="email"
                                id="verify-email"
                                name="email"
                                placeholder="you@example.com"
                                class="input input-bordered w-full"
                                required=(true)
                            />
                        </div>

                        <div>
                            <label class="label text-xs font-bold uppercase text-center w-full block text-base-content/70">"6-Digit Code"</label>
                            <input
                                type="text"
                                id="verify-code"
                                name="code"
                                placeholder="123456"
                                maxlength="6"
                                class="input input-bordered w-full text-center text-2xl tracking-[0.5em] font-mono font-bold"
                                required=(true)
                            />
                        </div>

                        <button id="btn-verify" type="submit" class="btn btn-primary w-full mt-4 font-bold flex items-center justify-center gap-2">
                            <span id="btn-verify-spinner" class="loading loading-spinner loading-xs hidden"></span>
                            <span id="btn-verify-text">"Verify & Continue"</span>
                        </button>
                    </form>

                    <div class="text-center text-xs text-base-content/60 pt-2 flex items-center justify-center gap-2">
                        <span>"Didn't receive code?"</span>
                        <button type="button" id="btn-resend-code" class="link link-primary font-bold">"Resend Code"</button>
                    </div>
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
                        if (emailEl) {
                            emailEl.value = cleanEmail;
                        }
                        if (infoText) {
                            infoText.innerText = 'We sent a 6-digit verification code to ' + cleanEmail + '. Please enter it below to activate your account.';
                        }
                        if (infoBox) {
                            infoBox.style.display = 'flex';
                        }
                        if (codeEl) {
                            codeEl.focus();
                        }
                    }
                } catch(e) {}

                var resendBtn = document.getElementById('btn-resend-code');
                if (resendBtn) {
                    resendBtn.onclick = function(e) {
                        if (e) {
                            try { e.preventDefault(); } catch(err) {}
                        }
                        if (errBox) errBox.style.display = 'none';

                        var email = '';
                        if (emailEl) {
                            if (emailEl.value) {
                                email = emailEl.value.trim();
                            }
                        }
                        if (!email) {
                            if (errText) errText.innerText = 'Please enter your email address to resend the code.';
                            if (errBox) errBox.style.display = 'flex';
                            return;
                        }

                        resendBtn.disabled = true;
                        resendBtn.innerText = 'Sending...';

                        fetch('http://localhost:8083/api/v1/users/resend-verification', {
                            method: 'POST',
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': 'application/json'
                            },
                            body: JSON.stringify({ email: email })
                        })
                        .then(function(res) {
                            resendBtn.disabled = false;
                            resendBtn.innerText = 'Resend Code';
                            if (res.ok) {
                                if (infoText) infoText.innerText = 'A new verification code has been sent to ' + email + '.';
                                if (infoBox) infoBox.style.display = 'flex';
                            } else {
                                return res.json().then(function(err) {
                                    var msg = 'Failed to resend code. Account may already be verified.';
                                    if (err) {
                                        if (err.message) {
                                            msg = err.message;
                                        }
                                    }
                                    if (errText) errText.innerText = msg;
                                    if (errBox) errBox.style.display = 'flex';
                                }).catch(function() {
                                    if (errText) errText.innerText = 'Failed to resend verification code.';
                                    if (errBox) errBox.style.display = 'flex';
                                });
                            }
                        })
                        .catch(function(err) {
                            resendBtn.disabled = false;
                            resendBtn.innerText = 'Resend Code';
                            if (errText) errText.innerText = 'Network error while requesting verification code.';
                            if (errBox) errBox.style.display = 'flex';
                        });
                    };
                }

                var form = document.getElementById('guest-verify-form');
                if (!form) return;
                form.onsubmit = function(e) {
                    if (e) {
                        try {
                            e.preventDefault();
                            e.stopPropagation();
                        } catch(err) {}
                    }

                    if (errBox) errBox.style.display = 'none';

                    var btn = document.getElementById('btn-verify');
                    var spinner = document.getElementById('btn-verify-spinner');
                    var btnText = document.getElementById('btn-verify-text');

                    var email = '';
                    if (emailEl) { if (emailEl.value) email = emailEl.value.trim(); }
                    var code = '';
                    if (codeEl) { if (codeEl.value) code = codeEl.value.trim(); }

                    if (!email) {
                        if (errText) errText.innerText = 'Please enter your email.';
                        if (errBox) errBox.style.display = 'flex';
                        return false;
                    }
                    if (!code) {
                        if (errText) errText.innerText = 'Please enter your 6-digit verification code.';
                        if (errBox) errBox.style.display = 'flex';
                        return false;
                    }

                    if (btn) btn.disabled = true;
                    if (spinner) spinner.classList.remove('hidden');
                    if (btnText) btnText.innerText = 'Verifying...';

                    fetch('http://localhost:8083/api/v1/users/verify', {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json',
                            'Accept': 'application/json'
                        },
                        body: JSON.stringify({ email: email, code: code })
                    })
                    .then(function(res) {
                        if (res.ok) {
                            return res.json().then(function(data) {
                                var u = data;
                                if (data.data) { u = data.data; }
                                var fn = '';
                                if (u.first_name) { fn = u.first_name; }
                                var ln = '';
                                if (u.last_name) { ln = u.last_name; }
                                var fullName = (fn + ' ' + ln).trim();
                                if (!fullName) { fullName = 'Verified Guest'; }
                                var emailStr = email;
                                if (u.email) { emailStr = u.email; }
                                var phoneStr = '';
                                if (u.phone_number) { phoneStr = u.phone_number; }
                                var idStr = '';
                                if (u.id) { idStr = u.id; }

                                if (window.loginUser) {
                                    window.loginUser(fullName, emailStr, 'booker', fn, ln, phoneStr, idStr);
                                } else {
                                    localStorage.setItem('op_auth_user', JSON.stringify({
                                        name: fullName,
                                        email: emailStr,
                                        role: 'booker',
                                        first_name: fn,
                                        last_name: ln,
                                        phone: phoneStr,
                                        id: idStr
                                    }));
                                    window.location.href = '/';
                                }
                            });
                        } else {
                            return res.json().then(function(err) {
                                if (btn) btn.disabled = false;
                                if (spinner) spinner.classList.add('hidden');
                                if (btnText) btnText.innerText = 'Verify & Continue';
                                var msg = 'Invalid verification code.';
                                if (err) {
                                    if (err.message) {
                                        msg = err.message;
                                    }
                                }
                                if (errText) errText.innerText = msg;
                                if (errBox) errBox.style.display = 'flex';
                            }).catch(function() {
                                if (btn) btn.disabled = false;
                                if (spinner) spinner.classList.add('hidden');
                                if (btnText) btnText.innerText = 'Verify & Continue';
                                if (errText) errText.innerText = 'Invalid verification code.';
                                if (errBox) errBox.style.display = 'flex';
                            });
                        }
                    })
                    .catch(function(err) {
                        console.error('user_api verify connection failed:', err);
                        if (btn) btn.disabled = false;
                        if (spinner) spinner.classList.add('hidden');
                        if (btnText) btnText.innerText = 'Verify & Continue';
                        if (errText) errText.innerText = 'Unable to connect to verification service.';
                        if (errBox) errBox.style.display = 'flex';
                    });

                    return false;
                };
            })();
            "#
        </script>
    })
}
