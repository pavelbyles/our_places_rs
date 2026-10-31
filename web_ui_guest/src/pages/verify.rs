use common::models::{ResendVerificationRequest, VerifyRequest};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page, parse_query_params},
    view::{View, component, view},
};
use web_client::get_api_client;

use crate::support::start_guest_session;

#[derive(Deserialize, Default)]
struct VerifyQuery {
    email: Option<String>,
    /// Set when sign-in sent an unverified account here.
    unverified: Option<String>,
}

/// Verification form fields.
#[derive(Deserialize)]
pub struct VerifyForm {
    /// The account email.
    pub email: String,
    /// The 6-character code from the email.
    #[serde(default)]
    pub code: String,
}

/// Resend form fields.
#[derive(Deserialize)]
pub struct ResendForm {
    /// The account email.
    pub email: String,
}

#[component]
async fn verify_form(email: String, info: String, error: Option<String>) -> Result<impl View> {
    let label = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";
    Ok(view! {
        <div class="mx-auto w-full max-w-md space-y-6 rounded-[var(--radius-surface,16px)] border border-border bg-card p-8 text-card-foreground">
            <div class="space-y-1 text-center">
                <h1 class="font-serif text-3xl">"Verify your email"</h1>
            </div>
            <div id="verify-info" role="status" class="rounded-lg border border-border p-3 text-sm">
                <span id="verify-info-text">(info)</span>
            </div>
            if let Some(error) = error {
                <div id="verify-error" role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">
                    <span id="verify-error-text">(error)</span>
                </div>
            }
            <form method="post" action="/verify" class="space-y-4">
                <label class=(label) for="verify-email">"Email address"</label>
                <input id="verify-email" name="email" type="email" required=(true) autocomplete="email" value=(email.clone()) class=(input) />
                <label class=(label) for="verify-code">"Verification code"</label>
                <input id="verify-code" name="code" required=(true) minlength="6" maxlength="6" autocomplete="one-time-code" autocapitalize="characters" class=(format!("{input} font-mono tracking-[0.3em] uppercase")) />
                <button id="btn-verify" type="submit" class="h-11 w-full rounded-[var(--radius-control,10px)] bg-primary text-[15px] font-medium text-primary-foreground">"Verify and sign in"</button>
            </form>
            <form method="post" action="/verify/resend" class="text-center">
                <input type="hidden" name="email" value=(email) />
                <button id="btn-resend-code" type="submit" class="text-sm font-medium text-link underline">"Send a new code"</button>
            </form>
        </div>
    })
}

fn sent_to(email: &str) -> String {
    if email.is_empty() {
        "Enter the 6-character code we emailed you.".to_string()
    } else {
        format!("We emailed a 6-character code to {email}.")
    }
}

/// Shown when sign-in finds the account unverified, e.g. one an administrator
/// created: its code may well have expired by the time they first sign in.
fn unverified_notice(email: &str) -> String {
    format!(
        "Your account isn't verified yet. Enter the 6-character code we emailed to {email}, \
         or send a new one if it has expired."
    )
}

/// The verification page (`?email=` pre-fills the address).
#[page("/verify")]
pub async fn verify_page(cx: &Cx) -> Result<impl View> {
    let q = parse_query_params::<VerifyQuery>(cx).unwrap_or_default();
    let email = q.email.unwrap_or_default();
    let info = if q.unverified.is_some() {
        unverified_notice(&email)
    } else {
        sent_to(&email)
    };
    Ok(view! { verify_form(email: email, info: info, error: None) })
}

/// Checks the code with `user_api` and signs the guest in, as the legacy
/// portal does after verification.
#[page(POST "/verify")]
pub async fn verify_submit(
    cx: &Cx,
    /// The submitted verification form.
    Form(form): Form<VerifyForm>,
) -> Result<impl View> {
    let request = VerifyRequest {
        email: form.email.trim().to_string(),
        code: form.code.trim().to_uppercase(),
    };
    let error = match get_api_client(cx).verify_user(&request).await {
        Ok(user) => match start_guest_session(cx, &user).await {
            Ok(()) => return Err(see_other("/").into()),
            Err(e) => {
                tracing::error!("{e}");
                // Verified, but no session: let them sign in normally.
                return Err(see_other("/login?verified=1").into());
            }
        },
        Err(e) => {
            tracing::info!("verification rejected: {e}");
            "Invalid or expired verification code.".to_string()
        }
    };
    let email = form.email;
    let info = sent_to(&email);
    Ok(view! { verify_form(email: email, info: info, error: Some(error)) })
}

/// Sends a fresh code.
#[page(POST "/verify/resend")]
pub async fn verify_resend(
    cx: &Cx,
    /// The submitted resend form.
    Form(form): Form<ResendForm>,
) -> Result<impl View> {
    let email = form.email.trim().to_string();
    let request = ResendVerificationRequest {
        email: email.clone(),
    };
    let (info, error) = match get_api_client(cx).resend_verification(&request).await {
        Ok(_) => (
            format!("A new verification code has been sent to {email}."),
            None,
        ),
        Err(e) => {
            tracing::info!("resend rejected: {e}");
            (
                sent_to(&email),
                Some("We couldn't send a new code. Check the address and try again.".to_string()),
            )
        }
    };
    Ok(view! { verify_form(email: email, info: info, error: error) })
}
