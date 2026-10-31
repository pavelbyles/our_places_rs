use common::models::{NewBookerProfile, NewUserRequest};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page},
    view::{View, component, view},
};
use web_client::get_api_client;

/// Registration form fields.
#[derive(Deserialize, Default, Clone)]
pub struct RegisterForm {
    /// First name.
    pub first_name: String,
    /// Last name.
    pub last_name: String,
    /// Email address.
    pub email: String,
    /// Password (at least 8 characters).
    #[serde(default)]
    pub password: String,
}

const MIN_PASSWORD_LEN: usize = 8;

#[component]
async fn register_form(form: RegisterForm, error: Option<String>) -> Result<impl View> {
    let label = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";
    Ok(view! {
        <div class="mx-auto w-full max-w-md space-y-6 rounded-[var(--radius-surface,16px)] border border-border bg-card p-8 text-card-foreground">
            <div class="space-y-1 text-center">
                <h1 class="sr-only">"Register"</h1>
                <h2 class="font-serif text-3xl">"Create Account"</h2>
                <p class="text-sm text-muted-foreground">"Book villas and keep every stay in one place."</p>
            </div>
            if let Some(error) = error {
                <div id="reg-error" role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(error)</div>
            }
            <form method="post" action="/register" class="space-y-4">
                <div class="grid grid-cols-2 gap-3">
                    <div>
                        <label class=(label) for="reg-first-name">"First name"</label>
                        <input id="reg-first-name" name="first_name" required=(true) autocomplete="given-name" value=(form.first_name) class=(input) />
                    </div>
                    <div>
                        <label class=(label) for="reg-last-name">"Last name"</label>
                        <input id="reg-last-name" name="last_name" required=(true) autocomplete="family-name" value=(form.last_name) class=(input) />
                    </div>
                </div>
                <label class=(label) for="reg-email">"Email address"</label>
                <input id="reg-email" name="email" type="email" required=(true) autocomplete="email" value=(form.email) class=(input) />
                <label class=(label) for="reg-password">"Password"</label>
                <input id="reg-password" name="password" type="password" required=(true) minlength="8" autocomplete="new-password" aria-describedby="reg-password-hint" class=(input) />
                <p id="reg-password-hint" class="text-xs text-muted-foreground">"At least 8 characters."</p>
                <button id="btn-guest-reg" type="submit" class="h-11 w-full rounded-[var(--radius-control,10px)] bg-primary text-[15px] font-medium text-primary-foreground">"Create account"</button>
            </form>
            <p class="text-center text-sm text-muted-foreground">"Already have an account? " <a href="/login">"Sign in"</a></p>
        </div>
    })
}

/// The guest registration page.
#[page("/register")]
pub async fn register_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! { register_form(form: RegisterForm::default(), error: None) })
}

/// Creates a booker account through `user_api` (server-side; the legacy page
/// called `localhost:8083` from the browser) and goes to verification.
#[page(POST "/register")]
pub async fn register_submit(
    cx: &Cx,
    /// The submitted registration form.
    Form(form): Form<RegisterForm>,
) -> Result<impl View> {
    let error = if form.password.chars().count() < MIN_PASSWORD_LEN {
        Some("Password must be at least 8 characters.".to_string())
    } else {
        let request = NewUserRequest {
            email: form.email.trim().to_string(),
            password: form.password.clone(),
            first_name: form.first_name.trim().to_string(),
            last_name: form.last_name.trim().to_string(),
            phone_number: None,
            is_active: true,
            is_verified: false,
            attributes: None,
            roles: Some(vec!["booker".to_string()]),
            booker_profile: Some(NewBookerProfile {
                emergency_contacts: None,
                booking_preferences: None,
                loyalty: None,
            }),
            host_profile: None,
            default_currency: None,
        };
        match get_api_client(cx).create_user(&request).await {
            Ok(user) => {
                let target = format!("/verify?email={}", urlencode(&user.email));
                return Err(see_other(target).into());
            }
            Err(e) => {
                tracing::info!("guest registration rejected: {e}");
                let text = e.to_string().to_lowercase();
                Some(
                    if text.contains("exist") || text.contains("duplicate") || text.contains("409")
                    {
                        "An account with this email already exists. Sign in instead.".to_string()
                    } else if text.contains("valid") || text.contains("400") {
                        "Please check your details and try again.".to_string()
                    } else {
                        "Registration is unavailable right now. Please try again shortly."
                            .to_string()
                    },
                )
            }
        }
    };
    let form = RegisterForm {
        password: String::new(),
        ..form
    };
    Ok(view! { register_form(form: form, error: error) })
}

/// Percent-encodes a query value.
pub fn urlencode(value: &str) -> String {
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
