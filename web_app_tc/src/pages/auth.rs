use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};

#[page("/login")]
pub async fn login_page(_cx: &Cx) -> Result {
    view! {
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
                        onsubmit="handleGuestLogin(event); return false;"
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
            function handleGuestLogin(e) {
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

                fetch('http://localhost:8083/api/v1/users/login', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Accept': 'application/json'
                    },
                    body: JSON.stringify({ email: email, password: pass })
                })
                .then(function(res) {
                    if (res.ok) {
                        return res.json().then(function(data) {
                            var u = data.data || data;
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
                        });
                    } else {
                        return res.json().then(function(err) {
                            if (btn) btn.disabled = false;
                            if (spinner) spinner.classList.add('hidden');
                            if (btnText) btnText.innerText = 'Log In';
                            if (errText) errText.innerText = err.message || 'Invalid email or password.';
                            if (errBox) errBox.style.display = 'flex';
                        }).catch(function() {
                            if (btn) btn.disabled = false;
                            if (spinner) spinner.classList.add('hidden');
                            if (btnText) btnText.innerText = 'Log In';
                            if (errText) errText.innerText = 'Invalid email or password.';
                            if (errBox) errBox.style.display = 'flex';
                        });
                    }
                })
                .catch(function(err) {
                    console.error('user_api connection failed:', err);
                    if (btn) btn.disabled = false;
                    if (spinner) spinner.classList.add('hidden');
                    if (btnText) btnText.innerText = 'Log In';
                    if (errText) errText.innerText = 'Unable to connect to authentication service. Please ensure the user API is running.';
                    if (errBox) errBox.style.display = 'flex';
                });

                return false;
            }
            "#
        </script>
    }
}

#[page("/register")]
pub async fn register_page(_cx: &Cx) -> Result {
    view! {
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
                        onsubmit="handleGuestRegister(event); return false;"
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
            function handleGuestRegister(e) {
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

                var firstName = (fnEl && fnEl.value) ? fnEl.value.trim() : '';
                var lastName = (lnEl && lnEl.value) ? lnEl.value.trim() : '';
                var email = (emailEl && emailEl.value) ? emailEl.value.trim() : '';
                var pass = (passEl && passEl.value) ? passEl.value.trim() : '';

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
                        roles: ['booker']
                    })
                })
                .then(function(res) {
                    if (res.ok) {
                        return res.json().then(function(data) {
                            var u = data.data || data;
                            var fullName = (u.first_name || firstName) + ' ' + (u.last_name || lastName);
                            fullName = fullName.trim();
                            if (!fullName) fullName = 'Guest User';
                            if (window.loginUser) {
                                window.loginUser(fullName, u.email || email, 'booker');
                            } else {
                                localStorage.setItem('op_auth_user', JSON.stringify({ name: fullName, email: u.email || email, role: 'booker' }));
                                window.location.href = '/';
                            }
                        });
                    } else {
                        return res.json().then(function(err) {
                            if (btn) btn.disabled = false;
                            if (spinner) spinner.classList.add('hidden');
                            if (btnText) btnText.innerText = 'Create Account';
                            if (errText) errText.innerText = err.message || 'Registration failed.';
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
            }
            "#
        </script>
    }
}

#[page("/verify")]
pub async fn verify_page(_cx: &Cx) -> Result {
    view! {
        <div class="min-h-[70vh] flex items-center justify-center py-10 px-4">
            <div class="card w-full max-w-md bg-base-100 shadow-xl border border-base-200 overflow-hidden rounded-3xl">
                <div class="bg-primary text-primary-content p-8 text-center">
                    <h2 class="text-3xl font-extrabold">"Verify Email"</h2>
                    <p class="mt-2 text-sm opacity-90">"Enter the 6-digit verification code sent to your email"</p>
                </div>

                <div class="p-8 space-y-6">
                    <div id="verify-error" class="alert alert-error text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                        <span>"⚠️"</span>
                        <span id="verify-error-text">"Verification error"</span>
                    </div>

                    <form
                        class="space-y-4"
                        onsubmit="handleEmailVerify(event); return false;"
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

                    <div class="text-center text-xs text-base-content/60 pt-2">
                        <span>"Didn't receive code? "</span>
                        <a href="#" class="link link-primary">"Resend Code"</a>
                    </div>
                </div>
            </div>
        </div>

        <script>
            r#"
            function handleEmailVerify(e) {
                if (e) {
                    try {
                        e.preventDefault();
                        e.stopPropagation();
                    } catch(err) {}
                }

                var errBox = document.getElementById('verify-error');
                var errText = document.getElementById('verify-error-text');
                if (errBox) errBox.style.display = 'none';

                var emailEl = document.getElementById('verify-email');
                var codeEl = document.getElementById('verify-code');
                var btn = document.getElementById('btn-verify');
                var spinner = document.getElementById('btn-verify-spinner');
                var btnText = document.getElementById('btn-verify-text');

                var email = (emailEl && emailEl.value) ? emailEl.value.trim() : '';
                var code = (codeEl && codeEl.value) ? codeEl.value.trim() : '';

                if (!email || !code) {
                    if (errText) errText.innerText = 'Please enter both your email and verification code.';
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
                            var u = data.data || data;
                            var fullName = (u.first_name || '') + ' ' + (u.last_name || '');
                            fullName = fullName.trim() || 'Verified Guest';
                            if (window.loginUser) {
                                window.loginUser(fullName, u.email || email, 'booker');
                            } else {
                                localStorage.setItem('op_auth_user', JSON.stringify({ name: fullName, email: u.email || email, role: 'booker' }));
                                window.location.href = '/';
                            }
                        });
                    } else {
                        return res.json().then(function(err) {
                            if (btn) btn.disabled = false;
                            if (spinner) spinner.classList.add('hidden');
                            if (btnText) btnText.innerText = 'Verify & Continue';
                            if (errText) errText.innerText = err.message || 'Invalid verification code.';
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
            }
            "#
        </script>
    }
}
