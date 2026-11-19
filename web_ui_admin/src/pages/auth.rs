use common::models::LoginRequest;
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page, parse_query_params},
    view::{View, component, view},
};
use web_client::{auth::token_hash_to_hex, get_api_client, session::require_admin};

use crate::pages::register::urlencode;
use crate::support::{portal_role, post_login_target, start_admin_session};

#[derive(Deserialize, Default)]
struct LoginQuery {
    redirect: Option<String>,
    reset: Option<String>,
}

/// Login form fields.
#[derive(Deserialize)]
pub struct LoginForm {
    /// Email address.
    pub email: String,
    /// Password.
    pub password: String,
    /// Where to go after signing in (same-origin paths only).
    #[serde(default)]
    pub redirect: String,
}

#[component]
pub(crate) async fn login_form(
    cx: &Cx,
    email: String,
    redirect: String,
    notice: Option<String>,
    error: Option<String>,
) -> Result<impl View> {
    let label = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";
    Ok(view! {
        <div
            class="w-full max-w-md space-y-6 rounded-xl border border-border bg-card p-8 text-card-foreground"
        >
            <div class="space-y-1">
                <h1 class="sr-only">"Console sign in"</h1>
                <h2 class="text-2xl font-semibold">"Sign in to Our Places"</h2>
                <p class="text-sm text-muted-foreground">
                    "For hosts, administrators, and cleaning staff."
                </p>
            </div>
            if let Some(notice) = notice {
                <div
                    id="admin-auth-notice"
                    role="status"
                    class="rounded-lg border border-border bg-muted/40 p-3 text-sm text-foreground"
                >
                    <span id="admin-auth-notice-text">(notice)</span>
                </div>
            }
            if let Some(error) = error {
                <div
                    id="admin-auth-error"
                    role="alert"
                    class="rounded-lg border border-destructive p-3 text-sm text-destructive"
                >
                    <span id="admin-auth-error-text">(error)</span>
                </div>
            }
            <form method="post" action="/login" class="space-y-4">
                <input type="hidden" name="redirect" value=(redirect) />
                <label class=(label) for="admin-login-email">"Email address"</label>
                <input
                    id="admin-login-email"
                    name="email"
                    type="email"
                    required=(true)
                    autocomplete="email"
                    value=(email)
                    class=(input)
                />
                <label class=(label) for="admin-login-password">"Password"</label>
                <input
                    id="admin-login-password"
                    name="password"
                    type="password"
                    required=(true)
                    autocomplete="current-password"
                    class=(input)
                />
                <div class="flex justify-end">
                    <a
                        id="admin-forgot-password-link"
                        href="/forgot-password"
                        class="text-sm text-muted-foreground underline hover:text-foreground"
                    >
                        "Forgot password?"
                    </a>
                </div>
                <button
                    id="btn-admin-login"
                    type="submit"
                    class="h-11 w-full rounded-lg bg-primary text-sm font-medium text-primary-foreground"
                >
                    "Sign in"
                </button>
            </form>
            <p class="text-center text-sm text-muted-foreground">
                "New host? "
                <a href="/register">"Create a host account"</a>
            </p>
        </div>
    })
}

/// The console sign-in page (signed-in users go straight to their portal landing page).
#[page("/login")]
pub async fn login_page(cx: &Cx) -> Result<impl View> {
    let query = parse_query_params::<LoginQuery>(cx).unwrap_or_default();
    let redirect = query.redirect.unwrap_or_default();
    if let Ok(session) = require_admin(cx).await {
        return Err(see_other(post_login_target(session.role, &redirect)).into());
    }
    let notice = (query.reset.as_deref() == Some("1"))
        .then(|| "Your password has been updated. Sign in with your new password.".to_string());
    Ok(view! { login_form(email: String::new(), redirect: redirect, notice: notice, error: None) })
}

/// Signs a host, staff member, or administrator in. Accounts without an allowed role are
/// refused with the same message as a wrong password. An unverified account
/// (the password was right) goes to the code prompt instead.
#[page(POST "/login")]
pub async fn login_submit(
    cx: &Cx,
    /// The submitted login form.
    Form(form): Form<LoginForm>,
) -> Result<impl View> {
    let request = LoginRequest {
        email: form.email.clone(),
        password: form.password,
    };
    let error = match get_api_client(cx).login_user(&request).await {
        Err(e) => {
            if e.to_string().to_lowercase().contains("not verified") {
                let target = format!("/verify?email={}&unverified=1", urlencode(&form.email));
                return Err(see_other(target).into());
            }
            tracing::info!("console sign-in rejected");
            "Invalid email or password. Access is restricted to hosts, staff, and administrators."
                .to_string()
        }
        Ok(user) => match portal_role(&user) {
            None => "Invalid email or password. Access is restricted to hosts, staff, and administrators."
                .to_string(),
            Some(role) => match start_admin_session(cx, &user, role).await {
                Ok(()) => {
                    let target = post_login_target(role, &form.redirect);
                    return Err(see_other(target).into());
                }
                Err(e) => {
                    tracing::error!("{e}");
                    "Sign-in failed. Please try again.".to_string()
                }
            },
        },
    };
    let (email, redirect) = (form.email, form.redirect);
    Ok(view! { login_form(email: email, redirect: redirect, notice: None, error: Some(error)) })
}

async fn end_session(cx: &Cx) -> Result<()> {
    if let Some(hash) = topcoat::session::stop(cx).await.ok().flatten()
        && let Err(e) = get_api_client(cx)
            .delete_session(&token_hash_to_hex(&hash))
            .await
    {
        tracing::warn!("failed to delete admin session: {e}");
    }
    Err(see_other("/login").into())
}

/// Signs out (GET kept for parity with the legacy console's link).
#[page("/logout")]
pub async fn logout_page(cx: &Cx) -> Result<()> {
    end_session(cx).await
}

/// Signs out from the top bar's form.
#[page(POST "/logout")]
pub async fn logout_submit(cx: &Cx) -> Result<()> {
    end_session(cx).await
}
