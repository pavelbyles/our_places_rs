//! Public `/forgot-password` and `/reset-password` flows for hosts, admins, and invited staff.

use common::turnover::{PasswordResetRequest, PasswordTokenRedeemRequest, is_valid_trimmed_email};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page, parse_query_params},
    view::{View, component, view},
};
use web_client::{client::UserCallError, get_api_client};

const MIN_PASSWORD_LEN: usize = 8;
const LABEL: &str = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
const INPUT: &str =
    "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";

#[derive(Deserialize, Default)]
struct ResetPasswordQuery {
    token: Option<String>,
}

/// Submitted forgot-password form.
#[derive(Deserialize, Clone)]
pub struct ForgotPasswordForm {
    /// Target account email address.
    pub email: String,
}

/// Submitted reset/setup password form.
#[derive(Deserialize, Clone)]
pub struct ResetPasswordForm {
    /// Opaque action token from the link query string.
    #[serde(default)]
    pub token: String,
    /// Chosen new password.
    #[serde(default)]
    pub new_password: String,
    /// Repeated password confirmation.
    #[serde(default)]
    pub confirm_password: String,
}

/// Render state for `/reset-password`.
#[derive(Clone, PartialEq, Eq)]
pub enum ResetCardState {
    /// Active password form with optional inline validation/backend error.
    Form {
        /// Opaque single-use token.
        token: String,
        /// Inline validation or submission error message.
        error: Option<String>,
    },
    /// Missing, invalid, or expired token: hides the form and prompts recovery.
    ExpiredOrInvalid,
}

#[component]
pub(crate) async fn forgot_password_card(
    cx: &Cx,
    email: String,
    sent: bool,
    error: Option<String>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class="w-full max-w-md space-y-6 rounded-xl border border-border bg-card p-8 text-card-foreground"
        >
            <div class="space-y-1">
                <h1 class="text-2xl font-semibold">"Reset your password"</h1>
                <p class="text-sm text-muted-foreground">
                    "Enter your email address and we will send a single-use password reset link."
                </p>
            </div>
            if sent {
                <div
                    id="forgot-password-sent"
                    role="status"
                    class="rounded-lg border border-border bg-muted/40 p-4 text-sm text-foreground"
                >
                    "If an active account exists for that email address, we have sent a password reset link. Links expire after 30 minutes."
                </div>
            }
            if let Some(error) = error {
                <div
                    id="forgot-password-error"
                    role="alert"
                    class="rounded-lg border border-destructive p-3 text-sm text-destructive"
                >
                    <span id="forgot-password-error-text">(error)</span>
                </div>
            }
            <form method="post" action="/forgot-password" class="space-y-4">
                <div>
                    <label class=(LABEL) for="forgot-password-email">"Email address"</label>
                    <input
                        id="forgot-password-email"
                        name="email"
                        type="email"
                        required=(true)
                        autocomplete="email"
                        value=(email)
                        class=(INPUT)
                    />
                </div>
                <button
                    id="btn-forgot-password-submit"
                    type="submit"
                    class="h-11 w-full rounded-lg bg-primary text-sm font-medium text-primary-foreground"
                >
                    "Send reset link"
                </button>
            </form>
            <p class="text-center text-sm text-muted-foreground">
                <a id="forgot-password-back-login" href="/login" class="underline">
                    "Back to sign in"
                </a>
            </p>
        </div>
    })
}

#[component]
pub(crate) async fn reset_password_card(cx: &Cx, state: ResetCardState) -> Result<impl View> {
    Ok(view! {
        <div
            class="w-full max-w-md space-y-6 rounded-xl border border-border bg-card p-8 text-card-foreground"
        >
            <div class="space-y-1">
                <h1 class="text-2xl font-semibold">"Set a new password"</h1>
                <p class="text-sm text-muted-foreground">
                    "Choose a strong password of at least 8 characters."
                </p>
            </div>
            if let ResetCardState::Form { token, error } = state {
                <div class="space-y-4">
                    if let Some(error) = error {
                        <div
                            id="reset-password-error"
                            role="alert"
                            class="rounded-lg border border-destructive p-3 text-sm text-destructive"
                        >
                            <span id="reset-password-error-text">(error)</span>
                        </div>
                    }
                    <form method="post" action="/reset-password" class="space-y-4">
                        <input type="hidden" name="token" value=(token) />
                        <div>
                            <label class=(LABEL) for="reset-new-password">"New password"</label>
                            <input
                                id="reset-new-password"
                                name="new_password"
                                type="password"
                                required=(true)
                                minlength="8"
                                autocomplete="new-password"
                                class=(INPUT)
                            />
                        </div>
                        <div>
                            <label class=(LABEL) for="reset-confirm-password">"Confirm new password"</label>
                            <input
                                id="reset-confirm-password"
                                name="confirm_password"
                                type="password"
                                required=(true)
                                minlength="8"
                                autocomplete="new-password"
                                class=(INPUT)
                            />
                        </div>
                        <button
                            id="btn-reset-password-submit"
                            type="submit"
                            class="h-11 w-full rounded-lg bg-primary text-sm font-medium text-primary-foreground"
                        >
                            "Save new password"
                        </button>
                    </form>
                    <p class="text-center text-sm text-muted-foreground">
                        <a href="/login" class="underline">"Back to sign in"</a>
                    </p>
                </div>
            } else {
                <div class="space-y-4">
                    <div
                        id="reset-password-expired"
                        role="alert"
                        class="space-y-2 rounded-lg border border-destructive p-4 text-sm text-destructive"
                    >
                        <p class="font-medium">"This password link is invalid or has expired."</p>
                        <p class="text-muted-foreground">
                            "For your security, setup and reset links can only be used once. Request a fresh link or ask your host or administrator to resend your invite."
                        </p>
                    </div>
                    <a
                        id="reset-password-request-new-link"
                        href="/forgot-password"
                        class="inline-flex h-11 w-full items-center justify-center rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground no-underline"
                    >
                        "Request a new reset link"
                    </a>
                    <p class="text-center text-sm text-muted-foreground">
                        <a href="/login" class="underline">"Back to sign in"</a>
                    </p>
                </div>
            }
        </div>
    })
}

/// Displays the public forgot-password form.
#[page("/forgot-password")]
pub async fn forgot_password_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! { forgot_password_card(email: String::new(), sent: false, error: None) })
}

/// Submits a forgot-password email request while keeping responses neutral.
#[page(POST "/forgot-password")]
pub async fn forgot_password_submit(
    cx: &Cx,
    /// Submitted email address.
    Form(form): Form<ForgotPasswordForm>,
) -> Result<impl View> {
    let raw_email = form.email.trim().to_string();
    let (email, sent, error) = if !is_valid_trimmed_email(&raw_email) {
        (
            raw_email,
            false,
            Some("Enter a valid email address.".to_string()),
        )
    } else {
        let payload = PasswordResetRequest {
            email: raw_email.clone(),
        };
        match get_api_client(cx).request_password_reset(&payload).await {
            Ok(()) => (String::new(), true, None),
            Err(UserCallError::Api(rej)) => (raw_email, false, Some(rej.message)),
            Err(err) => {
                tracing::error!(error = %err, "forgot password request failed");
                (
                    raw_email,
                    false,
                    Some("Unable to send reset link right now. Please try again.".to_string()),
                )
            }
        }
    };

    Ok(view! { forgot_password_card(email: email, sent: sent, error: error) })
}

/// Displays the single-use password setup/reset form when a non-empty token is present.
#[page("/reset-password")]
pub async fn reset_password_page(cx: &Cx) -> Result<impl View> {
    let token = parse_query_params::<ResetPasswordQuery>(cx)
        .ok()
        .and_then(|q| q.token)
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    let state = match token {
        Some(token) => ResetCardState::Form { token, error: None },
        None => ResetCardState::ExpiredOrInvalid,
    };
    Ok(view! { reset_password_card(state: state) })
}

/// Redeems the single-use password token and redirects back to sign in.
#[page(POST "/reset-password")]
pub async fn reset_password_submit(
    cx: &Cx,
    /// Submitted password reset form.
    Form(form): Form<ResetPasswordForm>,
) -> Result<impl View> {
    let token = form.token.trim().to_string();
    let state = if token.is_empty() {
        ResetCardState::ExpiredOrInvalid
    } else if form.new_password.len() < MIN_PASSWORD_LEN {
        ResetCardState::Form {
            token,
            error: Some(format!(
                "Password must be at least {MIN_PASSWORD_LEN} characters long."
            )),
        }
    } else if form.new_password != form.confirm_password {
        ResetCardState::Form {
            token,
            error: Some("Passwords do not match.".to_string()),
        }
    } else {
        let req = PasswordTokenRedeemRequest {
            token: token.clone(),
            new_password: form.new_password,
        };
        match get_api_client(cx).confirm_password_reset(&req).await {
            Ok(()) => return Err(see_other("/login?reset=1").into()),
            Err(UserCallError::Unauthorized) => ResetCardState::ExpiredOrInvalid,
            Err(UserCallError::Api(rej)) => ResetCardState::Form {
                token,
                error: Some(rej.message),
            },
            Err(err) => {
                tracing::error!(error = %err, "password reset redemption failed");
                ResetCardState::Form {
                    token,
                    error: Some(
                        "Could not update your password right now. Please try again.".to_string(),
                    ),
                }
            }
        }
    };

    Ok(view! { reset_password_card(state: state) })
}
