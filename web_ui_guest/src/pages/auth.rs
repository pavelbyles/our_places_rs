use common::models::{CreateSessionRequest, LoginRequest};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page, parse_query_params},
    view::{View, component, view},
};
use web_client::{
    auth::token_hash_to_hex, get_api_client, redirect::safe_redirect, session::GUEST_NAMESPACE,
};

/// Guest sessions last 7 days, refreshed on use (same as the legacy portal).
const SESSION_TTL_SECONDS: i64 = 604_800;

#[derive(Deserialize, Default)]
struct LoginQuery {
    redirect: Option<String>,
    verified: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginError {
    Invalid,
    NotVerified,
    Unavailable,
}

impl LoginError {
    fn message(&self) -> &'static str {
        match self {
            Self::Invalid => "Invalid email or password.",
            Self::NotVerified => "Your account is not verified yet. Enter the code we emailed you.",
            Self::Unavailable => "Sign-in is unavailable right now. Please try again shortly.",
        }
    }
}

#[component]
async fn login_form(
    email: String,
    redirect: String,
    error: Option<LoginError>,
    verified: bool,
) -> Result<impl View> {
    let not_verified = matches!(error, Some(LoginError::NotVerified));
    let verify_href = format!("/verify?email={}", email.replace('&', "%26"));
    let field = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";
    Ok(view! {
        <div class="mx-auto w-full max-w-md space-y-6 rounded-[var(--radius-surface,16px)] border border-border bg-card p-8 text-card-foreground">
            <div class="space-y-1 text-center">
                <h1 class="sr-only">"Sign in"</h1>
                <h2 class="font-serif text-3xl">"Welcome Back"</h2>
                <p class="text-sm text-muted-foreground">"Sign in to manage your bookings and stays."</p>
            </div>
            if verified {
                <p role="status" class="rounded-lg border border-border p-3 text-sm">"Your email is verified. Sign in to continue."</p>
            }
            if let Some(err) = error {
                <div id="guest-login-error" role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">
                    <span id="guest-login-error-text">(err.message())</span>
                    if not_verified {
                        " " <a href=(verify_href.clone())>"Verify your account"</a>
                    }
                </div>
            }
            <form id="guest-login-form" method="post" action="/login" class="space-y-4">
                <input type="hidden" name="redirect" value=(redirect) />
                <label class=(field) for="guest-email">"Email address"</label>
                <input id="guest-email" name="email" type="email" required=(true) autocomplete="email" value=(email) class=(input) />
                <label class=(field) for="guest-password">"Password"</label>
                <input id="guest-password" name="password" type="password" required=(true) autocomplete="current-password" class=(input) />
                <button id="btn-guest-login" type="submit" class="h-11 w-full rounded-[var(--radius-control,10px)] bg-primary text-[15px] font-medium text-primary-foreground">"Sign in"</button>
            </form>
            <p class="text-center text-sm text-muted-foreground">
                "Have a verification code? " <a href="/verify">"Verify your email"</a>
            </p>
            <p class="text-center text-sm text-muted-foreground">
                "New to Our Places? " <a href="/register">"Create an account"</a>
            </p>
        </div>
    })
}

/// The guest sign-in page.
#[page("/login")]
pub async fn login_page(cx: &Cx) -> Result<impl View> {
    let q = parse_query_params::<LoginQuery>(cx).unwrap_or_default();
    let redirect = q.redirect.unwrap_or_default();
    let verified = q.verified.is_some();
    Ok(
        view! { login_form(email: String::new(), redirect: redirect, error: None, verified: verified) },
    )
}

/// Signs a guest in: verifies the credentials with `user_api`, replaces any
/// existing session (fixation defence) and stores the new one in the
/// `guest` namespace, then redirects to a same-origin `redirect`.
#[page(POST "/login")]
pub async fn login_submit(
    cx: &Cx,
    /// The submitted login form.
    Form(form): Form<LoginForm>,
) -> Result<impl View> {
    let api = get_api_client(cx);
    let login = LoginRequest {
        email: form.email.clone(),
        password: form.password,
    };
    let error = match api.login_user(&login).await {
        Err(e) => {
            let text = e.to_string();
            tracing::info!("guest sign-in rejected");
            Some(
                if text.contains("not verified") || text.contains("Not verified") {
                    LoginError::NotVerified
                } else {
                    LoginError::Invalid
                },
            )
        }
        Ok(user) => {
            // Drop any previous session before issuing a new token.
            if let Some(old) = topcoat::session::stop(cx).await.ok().flatten() {
                let _ = api.delete_session(&token_hash_to_hex(&old)).await;
            }
            match topcoat::session::start(cx).await {
                Ok(session) => {
                    let name = format!("{} {}", user.first_name, user.last_name)
                        .trim()
                        .to_string();
                    let stored = api
                        .create_session(&CreateSessionRequest {
                            token_hash: token_hash_to_hex(&session.token_hash),
                            user_id: user.id,
                            email: user.email.clone(),
                            name: if name.is_empty() {
                                user.email.clone()
                            } else {
                                name
                            },
                            role: "guest".to_string(),
                            namespace: GUEST_NAMESPACE.to_string(),
                            ttl_seconds: SESSION_TTL_SECONDS,
                        })
                        .await;
                    match stored {
                        Ok(_) => return Err(see_other(safe_redirect(&form.redirect)).into()),
                        Err(e) => {
                            tracing::error!("failed to store guest session: {e}");
                            let _ = topcoat::session::stop(cx).await;
                            Some(LoginError::Unavailable)
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("failed to start guest session: {e}");
                    Some(LoginError::Unavailable)
                }
            }
        }
    };
    let (email, redirect) = (form.email, form.redirect);
    Ok(view! { login_form(email: email, redirect: redirect, error: error, verified: false) })
}

async fn end_session(cx: &Cx) -> Result<()> {
    if let Some(hash) = topcoat::session::stop(cx).await.ok().flatten()
        && let Err(e) = get_api_client(cx)
            .delete_session(&token_hash_to_hex(&hash))
            .await
    {
        tracing::warn!("failed to delete guest session: {e}");
    }
    Err(see_other("/login").into())
}

/// Signs out (kept as GET for parity with the legacy portal's link).
#[page("/logout")]
pub async fn logout_page(cx: &Cx) -> Result<impl View> {
    end_session(cx).await?;
    Ok(view! {})
}

/// Signs out from the header's form.
#[page(POST "/logout")]
pub async fn logout_submit(cx: &Cx) -> Result<impl View> {
    end_session(cx).await?;
    Ok(view! {})
}
