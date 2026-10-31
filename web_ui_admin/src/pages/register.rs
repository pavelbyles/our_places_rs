use common::models::{
    NewBookerProfile, NewHostProfile, NewUserRequest, ResendVerificationRequest, VerifyRequest,
};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page, parse_query_params},
    view::{View, component, view},
};
use web_client::get_api_client;

use crate::support::{portal_role, start_admin_session};

const MIN_PASSWORD_LEN: usize = 8;

/// Host registration form fields.
#[derive(Deserialize, Default, Clone)]
pub struct HostForm {
    /// First name.
    pub first_name: String,
    /// Last name.
    pub last_name: String,
    /// Email address.
    pub email: String,
    /// Phone number.
    #[serde(default)]
    pub phone_number: String,
    /// Password.
    #[serde(default)]
    pub password: String,
}

/// Verification form fields.
#[derive(Deserialize)]
pub struct VerifyForm {
    /// The account email.
    pub email: String,
    /// The emailed code.
    #[serde(default)]
    pub code: String,
}

#[derive(Deserialize, Default)]
struct EmailQuery {
    email: Option<String>,
    /// Set when sign-in sent an unverified account here.
    unverified: Option<String>,
}

/// Percent-encodes a query-string value.
pub(crate) fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

const LABEL: &str = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
const INPUT: &str =
    "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";

#[component]
async fn host_form(form: HostForm, error: Option<String>) -> Result<impl View> {
    Ok(view! {
        <div class="w-full max-w-lg space-y-6 rounded-xl border border-border bg-card p-8 text-card-foreground">
            <div class="space-y-1">
                <h1 class="text-2xl font-semibold">"Create a host account"</h1>
                <p class="text-sm text-muted-foreground">"List your villa and manage bookings. Administrator accounts are created by existing administrators."</p>
            </div>
            if let Some(error) = error {
                <div id="reg-error" role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive"><span id="reg-error-text">(error)</span></div>
            }
            <form id="page-host-signup-form" method="post" action="/register" class="space-y-4">
                <div class="grid grid-cols-2 gap-3">
                    <div><label class=(LABEL) for="page-host-fn">"First name"</label><input id="page-host-fn" name="first_name" required=(true) autocomplete="given-name" value=(form.first_name) class=(INPUT) /></div>
                    <div><label class=(LABEL) for="page-host-ln">"Last name"</label><input id="page-host-ln" name="last_name" required=(true) autocomplete="family-name" value=(form.last_name) class=(INPUT) /></div>
                </div>
                <label class=(LABEL) for="page-host-email">"Email address"</label>
                <input id="page-host-email" name="email" type="email" required=(true) autocomplete="email" value=(form.email) class=(INPUT) />
                <label class=(LABEL) for="page-host-phone">"Phone (optional)"</label>
                <input id="page-host-phone" name="phone_number" type="tel" autocomplete="tel" value=(form.phone_number) class=(INPUT) />
                <label class=(LABEL) for="page-host-pass">"Password"</label>
                <input id="page-host-pass" name="password" type="password" required=(true) minlength="8" autocomplete="new-password" class=(INPUT) />
                <button id="btn-page-signup" type="submit" class="h-11 w-full rounded-lg bg-primary text-sm font-medium text-primary-foreground">"Create host account"</button>
            </form>
            <p class="text-center text-sm text-muted-foreground">"Already registered? " <a href="/login">"Sign in"</a></p>
        </div>
    })
}

/// Public host registration.
#[page("/register")]
pub async fn register_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! { host_form(form: HostForm::default(), error: None) })
}

/// Registers a host (roles `host` and `booker`; never `admin`) through
/// `user_api`, then asks for the emailed code.
#[page(POST "/register")]
pub async fn register_submit(
    cx: &Cx,
    /// The registration form.
    Form(form): Form<HostForm>,
) -> Result<impl View> {
    let error = if form.password.chars().count() < MIN_PASSWORD_LEN {
        "Password must be at least 8 characters.".to_string()
    } else {
        let request = NewUserRequest {
            email: form.email.trim().to_string(),
            password: form.password.clone(),
            first_name: form.first_name.trim().to_string(),
            last_name: form.last_name.trim().to_string(),
            phone_number: Some(form.phone_number.trim().to_string()).filter(|p| !p.is_empty()),
            is_active: true,
            is_verified: false,
            attributes: None,
            roles: Some(vec!["host".to_string(), "booker".to_string()]),
            booker_profile: Some(NewBookerProfile {
                emergency_contacts: None,
                booking_preferences: None,
                loyalty: None,
            }),
            host_profile: Some(NewHostProfile {
                verified_status: Some("pending".to_string()),
                payout_details: None,
                description: None,
            }),
            default_currency: None,
        };
        match get_api_client(cx).create_user(&request).await {
            Ok(user) => {
                return Err(see_other(format!("/verify?email={}", urlencode(&user.email))).into());
            }
            Err(e) => {
                tracing::info!("host registration rejected: {e}");
                let text = e.to_string().to_lowercase();
                if text.contains("exist") || text.contains("duplicate") || text.contains("409") {
                    "An account with this email already exists. Sign in instead.".to_string()
                } else {
                    "Please check your details and try again.".to_string()
                }
            }
        }
    };
    let form = HostForm {
        password: String::new(),
        ..form
    };
    Ok(view! { host_form(form: form, error: Some(error)) })
}

#[component]
async fn verify_form(email: String, info: String, error: Option<String>) -> Result<impl View> {
    Ok(view! {
        <div class="w-full max-w-md space-y-6 rounded-xl border border-border bg-card p-8 text-card-foreground">
            <h1 class="text-2xl font-semibold">"Verify your email"</h1>
            <div id="verify-info" role="status" class="rounded-lg border border-border p-3 text-sm"><span id="verify-info-text">(info)</span></div>
            if let Some(error) = error {
                <div id="verify-error" role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive"><span id="verify-error-text">(error)</span></div>
            }
            <form id="host-verify-form" method="post" action="/verify" class="space-y-4">
                <label class=(LABEL) for="verify-email">"Email address"</label>
                <input id="verify-email" name="email" type="email" required=(true) value=(email.clone()) class=(INPUT) />
                <label class=(LABEL) for="verify-code">"Verification code"</label>
                <input id="verify-code" name="code" required=(true) minlength="6" maxlength="6" autocomplete="one-time-code" class=(format!("{INPUT} font-mono tracking-[0.3em] uppercase")) />
                <button id="btn-verify" type="submit" class="h-11 w-full rounded-lg bg-primary text-sm font-medium text-primary-foreground">"Verify and sign in"</button>
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
        "Enter the 6-character code we emailed you.".into()
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
    let q = parse_query_params::<EmailQuery>(cx).unwrap_or_default();
    let email = q.email.unwrap_or_default();
    let info = if q.unverified.is_some() {
        unverified_notice(&email)
    } else {
        sent_to(&email)
    };
    Ok(view! { verify_form(email: email, info: info, error: None) })
}

/// Checks the code and signs the host in to the console.
#[page(POST "/verify")]
pub async fn verify_submit(
    cx: &Cx,
    /// The verification form.
    Form(form): Form<VerifyForm>,
) -> Result<impl View> {
    let request = VerifyRequest {
        email: form.email.trim().to_string(),
        code: form.code.trim().to_uppercase(),
    };
    let error = match get_api_client(cx).verify_user(&request).await {
        Ok(user) => {
            // Verified, but only hosts and administrators get a console
            // session; sign-in then refuses anyone else as usual.
            let Some(role) = portal_role(&user) else {
                return Err(see_other("/login").into());
            };
            if let Err(e) = start_admin_session(cx, &user, role).await {
                tracing::error!("{e}");
                return Err(see_other("/login").into());
            }
            return Err(see_other("/admin").into());
        }
        Err(e) => {
            tracing::info!("host verification rejected: {e}");
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
    /// The resend form (email only).
    Form(form): Form<VerifyForm>,
) -> Result<impl View> {
    let email = form.email.trim().to_string();
    let (info, error) = match get_api_client(cx)
        .resend_verification(&ResendVerificationRequest {
            email: email.clone(),
        })
        .await
    {
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
