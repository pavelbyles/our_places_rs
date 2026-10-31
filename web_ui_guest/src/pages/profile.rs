use common::models::{UpdateUserRequest, UserResponse};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page, parse_query_params},
    view::{View, component, view},
};
use web_client::get_api_client;
use web_ui_common::components::domain::currency_selector::DISPLAY_CURRENCIES;

use crate::support::guest;

/// Editable profile fields.
#[derive(Deserialize)]
pub struct ProfileForm {
    /// First name.
    pub first_name: String,
    /// Last name.
    pub last_name: String,
    /// Phone number.
    #[serde(default)]
    pub phone_number: String,
    /// Preferred display currency.
    #[serde(default)]
    pub default_currency: String,
    /// Public handle used in `/listings?host={slug}`.
    #[serde(default)]
    pub slug: String,
}

#[derive(Deserialize, Default)]
struct ProfileQuery {
    saved: Option<String>,
}

#[component]
async fn profile_view(user: UserResponse, saved: bool, error: Option<String>) -> Result<impl View> {
    let label = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "mt-1 block h-11 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground normal-case tracking-normal font-normal";
    let currency = user.default_currency.clone();
    Ok(view! {
        <div class="mx-auto max-w-2xl space-y-6">
            <div class="space-y-1">
                <h1 class="font-serif text-[clamp(28px,4vw,40px)]">"Your profile"</h1>
                <p class="text-sm text-muted-foreground">(user.email.clone())</p>
            </div>
            if saved {
                <p role="status" class="rounded-lg border border-border p-3 text-sm">"Profile saved."</p>
            }
            if let Some(error) = error {
                <p role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(error)</p>
            }
            <form method="post" action="/profile" class="space-y-4 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6">
                <div class="grid gap-4 sm:grid-cols-2">
                    <div>
                        <label class=(label) for="profile-first-name">"First name"</label>
                        <input id="profile-first-name" name="first_name" required=(true) autocomplete="given-name" value=(user.first_name.clone()) class=(input) />
                    </div>
                    <div>
                        <label class=(label) for="profile-last-name">"Last name"</label>
                        <input id="profile-last-name" name="last_name" required=(true) autocomplete="family-name" value=(user.last_name.clone()) class=(input) />
                    </div>
                </div>
                <div>
                    <label class=(label) for="profile-phone">"Phone"</label>
                    <input id="profile-phone" name="phone_number" type="tel" autocomplete="tel" value=(user.phone_number.clone().unwrap_or_default()) class=(input) />
                </div>
                <div>
                    <label class=(label) for="profile-slug">"Profile URL"</label>
                    <div class="mt-1 flex items-center overflow-hidden rounded-lg border border-border bg-card text-sm focus-within:ring-2 focus-within:ring-primary">
                        <span class="shrink-0 pl-3 text-muted-foreground">"/listings?host="</span>
                        <input id="profile-slug" name="slug" required=(true) maxlength=(common::slug::MAX_SLUG_CHARS.to_string()) autocapitalize="none" spellcheck="false" aria-describedby="profile-slug-hint" value=(user.slug.clone()) class="h-11 min-w-0 flex-1 bg-transparent pr-3 text-foreground outline-none" />
                    </div>
                    <p id="profile-slug-hint" class="mt-1 text-xs text-muted-foreground">"Letters, numbers and hyphens. Links to your listings use it."</p>
                </div>
                <div>
                    <label class=(label) for="profile-currency">"Preferred currency"</label>
                    <select id="profile-currency" name="default_currency" class=(input)>
                        for c in DISPLAY_CURRENCIES {
                            <option value=(c) selected=(currency == c)>(c)</option>
                        }
                    </select>
                </div>
                <button type="submit" class="h-11 rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground">"Save changes"</button>
            </form>
        </div>
    })
}

/// The signed-in guest's profile.
#[page("/profile")]
pub async fn profile_page(cx: &Cx) -> Result<impl View> {
    let guest = guest(cx).await?;
    let user = get_api_client(cx)
        .get_user_profile(guest.user_id)
        .await
        .map_err(web_client::app_error)?;
    let saved = parse_query_params::<ProfileQuery>(cx)
        .ok()
        .and_then(|q| q.saved)
        .is_some();
    Ok(view! { profile_view(user: user, saved: saved, error: None) })
}

/// Saves the guest's own profile. The user ID always comes from the session.
#[page(POST "/profile")]
pub async fn profile_submit(
    cx: &Cx,
    /// The profile form.
    Form(form): Form<ProfileForm>,
) -> Result<impl View> {
    let guest = guest(cx).await?;
    let api = get_api_client(cx);
    let first = form.first_name.trim().to_string();
    let last = form.last_name.trim().to_string();
    let slug = common::slug::user_slug(&form.slug);
    let error = if first.is_empty() || last.is_empty() {
        Some("First and last name are required.".to_string())
    } else if let Err(message) = &slug {
        Some(message.clone())
    } else {
        let slug = slug.ok();
        let request = UpdateUserRequest {
            first_name: Some(first),
            last_name: Some(last),
            phone_number: Some(form.phone_number.trim().to_string()).filter(|p| !p.is_empty()),
            default_currency: DISPLAY_CURRENCIES
                .contains(&form.default_currency.as_str())
                .then(|| form.default_currency.clone()),
            email: None,
            password: None,
            is_active: None,
            is_verified: None,
            attributes: None,
            roles: None,
            booker_profile: None,
            host_profile: None,
            slug: slug.clone(),
        };
        match api.update_user(guest.user_id, &request).await {
            Ok(_) => return Err(see_other("/profile?saved=1").into()),
            Err(e) if e.to_string().contains("already taken") => Some(format!(
                "“{}” is already taken. Try another profile URL.",
                slug.unwrap_or_default()
            )),
            Err(e) => {
                tracing::info!("profile update rejected: {e}");
                Some("Your profile could not be saved. Please try again.".to_string())
            }
        }
    };
    let user = api
        .get_user_profile(guest.user_id)
        .await
        .map_err(web_client::app_error)?;
    Ok(view! { profile_view(user: user, saved: false, error: error) })
}
