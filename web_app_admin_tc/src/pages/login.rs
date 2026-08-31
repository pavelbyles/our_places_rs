use topcoat::{Result, context::Cx, router::page, view::view};

#[page("/login")]
pub async fn login_page(_cx: &Cx) -> Result {
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

                // Error Message Container
                <div id="admin-login-error" class="alert alert-error text-xs py-2.5 px-4 rounded-2xl font-semibold shadow-sm items-center gap-2" style="display: none;">
                    <span>"⚠️"</span>
                    <span id="admin-login-error-text">"Authentication error occurred"</span>
                </div>

                <form
                    class="mt-6 space-y-5"
                    id="admin-login-form"
                    onsubmit="window.handleAdminLogin(event); return false;"
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
                            type="submit"
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
                        "Protected by Google Cloud IAM & JWT Authentication"
                    </p>
                </div>
            </div>
        </div>

        <script>
            r#"
            (function() {
                var redirectTarget = '/';
                try {
                    var search = window.location.search;
                    if (search) {
                        var params = new URLSearchParams(search);
                        var r = params.get('redirect');
                        if (r) {
                            redirectTarget = decodeURIComponent(r);
                        }
                    }
                } catch(e) {}

                function showError(msg) {
                    var errBox = document.getElementById('admin-login-error');
                    var errText = document.getElementById('admin-login-error-text');
                    if (errText) {
                        errText.innerText = msg;
                    }
                    if (errBox) {
                        errBox.style.display = 'flex';
                    }
                }

                function hideError() {
                    var errBox = document.getElementById('admin-login-error');
                    if (errBox) {
                        errBox.style.display = 'none';
                    }
                }

                function setButtonLoading(isLoading) {
                    var btn = document.getElementById('btn-admin-login');
                    var spinner = document.getElementById('btn-admin-login-spinner');
                    var text = document.getElementById('btn-admin-login-text');
                    var arrow = document.getElementById('btn-admin-login-arrow');
                    
                    if (btn) {
                        btn.disabled = isLoading;
                        if (isLoading) {
                            btn.classList.add('opacity-80', 'cursor-not-allowed');
                        } else {
                            btn.classList.remove('opacity-80', 'cursor-not-allowed');
                        }
                    }
                    if (spinner) {
                        if (isLoading) {
                            spinner.classList.remove('hidden');
                        } else {
                            spinner.classList.add('hidden');
                        }
                    }
                    if (text) {
                        text.innerText = isLoading ? 'Authenticating...' : 'Sign In to Dashboard';
                    }
                    if (arrow) {
                        if (isLoading) {
                            arrow.classList.add('hidden');
                        } else {
                            arrow.classList.remove('hidden');
                        }
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

                    // Start spinner & disable submit
                    setButtonLoading(true);

                    // 1. Dispatch live HTTP authentication to user_api (port 8083)
                    fetch('http://localhost:8083/api/v1/users/login', {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json',
                            'Accept': 'application/json'
                        },
                        body: JSON.stringify({ email: email, password: password })
                    })
                    .then(function(response) {
                        if (response.ok) {
                            return response.json().then(function(payload) {
                                var userObj = payload.data || payload;
                                var fullName = '';
                                if (userObj.first_name) {
                                    fullName = userObj.first_name;
                                    if (userObj.last_name) {
                                        fullName = fullName + ' ' + userObj.last_name;
                                    }
                                }
                                if (!fullName) {
                                    fullName = userObj.name || userObj.email || 'Administrator';
                                }
                                
                                var primaryRole = 'superadmin';
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

                                var target = redirectTarget;
                                if (!target) {
                                    target = '/';
                                }
                                window.location.href = target;
                            });
                        } else {
                            return response.json().then(function(errJson) {
                                setButtonLoading(false);
                                showError(errJson.message || 'Invalid email or password. Access restricted to authorized administrators.');
                            }).catch(function() {
                                setButtonLoading(false);
                                showError('Invalid email or password. Access restricted to authorized administrators.');
                            });
                        }
                    })
                    .catch(function(networkErr) {
                        console.error('user_api connection failed:', networkErr);
                        setButtonLoading(false);
                        showError('Unable to connect to authentication service (user_api). Please ensure user_api is running.');
                    });

                    return false;
                };

                var loginBtn = document.getElementById('btn-admin-login');
                if (loginBtn) {
                    loginBtn.addEventListener('click', window.handleAdminLogin);
                }
                var loginForm = document.getElementById('admin-login-form');
                if (loginForm) {
                    loginForm.addEventListener('submit', window.handleAdminLogin);
                }
            })();
            "#
        </script>
    }
}
