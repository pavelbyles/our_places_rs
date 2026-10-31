use std::str::FromStr;

use common::models::{NewBookerProfile, NewHostProfile, NewUserRequest, UpdateUserRequest};
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{not_found, see_other},
        header::{HeaderName, HeaderValue},
        page, path_param,
        response::response_headers,
    },
    view::{View, component, view},
};
use uuid::Uuid;
use web_client::get_api_client;
use web_ui_common::components::domain::currency_selector::DISPLAY_CURRENCIES;

use super::{INPUT, LABEL, UserForm};
use crate::support::admin_only;

path_param!(id);

/// Validation or service errors above a form.
#[component]
async fn form_errors(
    /// Messages to list.
    errors: Vec<String>,
) -> Result<impl View> {
    Ok(view! {
        if !errors.is_empty() {
            <ul id="user-form-errors" role="alert" class="list-disc space-y-1 rounded-lg border border-destructive p-3 pl-6 text-sm text-destructive">
                for e in errors { <li>(e)</li> }
            </ul>
        }
    })
}

/// The fields shared by the create form and the edit dialog. `prefix` keeps
/// element IDs unique (`new-user` or `edit-user`).
#[component]
async fn user_fields(
    /// Current values.
    form: UserForm,
    /// ID prefix for inputs.
    prefix: String,
    /// ID prefix for role checkboxes (`role` or `edit-role`).
    role_prefix: String,
    /// Whether this creates an account (password required, no verified box).
    creating: bool,
) -> Result<impl View> {
    let id = move |name: &str| format!("{prefix}-{name}");
    let roles = [
        ("admin", "Administrator", form.role_admin.is_some()),
        ("host", "Host", form.role_host.is_some()),
        ("booker", "Guest (booker)", form.role_booker.is_some()),
    ];
    let caps = [
        (
            "can_manage_listings",
            "Manage listings",
            form.can_manage_listings.is_some(),
        ),
        (
            "can_manage_bookings",
            "Manage bookings",
            form.can_manage_bookings.is_some(),
        ),
        (
            "can_configure_rates",
            "Configure rates (admins only)",
            form.can_configure_rates.is_some(),
        ),
        (
            "can_manage_users",
            "Manage users (admins only)",
            form.can_manage_users.is_some(),
        ),
    ];
    let password_hint = if creating {
        "Password"
    } else {
        "New password (leave blank to keep)"
    };
    Ok(view! {
        <fieldset class="grid gap-4 sm:grid-cols-2">
            <legend class="mb-2 text-sm font-semibold">"Identity and contact"</legend>
            <div><label class=(LABEL) for=(id("first-name"))>"First name"</label>
                <input id=(id("first-name")) name="first_name" required=(true) value=(form.first_name.clone()) class=(INPUT) /></div>
            <div><label class=(LABEL) for=(id("last-name"))>"Last name"</label>
                <input id=(id("last-name")) name="last_name" required=(true) value=(form.last_name.clone()) class=(INPUT) /></div>
            <div><label class=(LABEL) for=(id("email"))>"Email"</label>
                <input id=(id("email")) name="email" type="email" required=(true) autocomplete="off" value=(form.email.clone()) class=(INPUT) /></div>
            <div><label class=(LABEL) for=(id("phone"))>"Phone (optional)"</label>
                <input id=(id("phone")) name="phone_number" type="tel" value=(form.phone_number.clone()) class=(INPUT) /></div>
            <div><label class=(LABEL) for=(id("password"))>(password_hint)</label>
                <input id=(id("password")) name="password" type="password" minlength="8" required=(creating) autocomplete="new-password" class=(INPUT) /></div>
            if !creating {
                <div class="sm:col-span-2"><label class=(LABEL) for=(id("slug"))>"Profile URL"</label>
                    <div class="mt-1 flex items-center overflow-hidden rounded-lg border border-border bg-card text-sm focus-within:ring-2 focus-within:ring-primary">
                        <span class="shrink-0 pl-3 text-muted-foreground">"/listings?host="</span>
                        <input id=(id("slug")) name="slug" required=(true) maxlength=(common::slug::MAX_SLUG_CHARS.to_string()) autocapitalize="none" spellcheck="false" value=(form.slug.clone()) class="h-10 min-w-0 flex-1 bg-transparent pr-3 text-foreground outline-none" />
                    </div></div>
            }
            <div><label class=(LABEL) for=(id("currency"))>"Display currency"</label>
                <select id=(id("currency")) name="default_currency" class=(INPUT)>
                    for c in DISPLAY_CURRENCIES {
                        <option value=(c) selected=(form.default_currency == c)>(c)</option>
                    }
                </select></div>
        </fieldset>
        <fieldset class="space-y-2">
            <legend class="mb-2 text-sm font-semibold">"Roles"</legend>
            <div class="flex flex-wrap gap-4">
                for (name, label, checked) in roles {
                    <label class="flex items-center gap-2 text-sm">
                        <input type="checkbox" id=(format!("{role_prefix}-{name}")) name=(format!("role_{name}")) checked=(checked) class="size-4" />(label)
                    </label>
                }
            </div>
        </fieldset>
        <fieldset class="space-y-2">
            <legend class="mb-2 text-sm font-semibold">"Capabilities"</legend>
            <p class="text-xs text-muted-foreground">"Guests hold none; hosts may manage only listings and bookings."</p>
            <div class="grid gap-2 sm:grid-cols-2">
                for (name, label, checked) in caps {
                    <label class="flex items-center gap-2 text-sm">
                        <input type="checkbox" id=(id(&name.replace('_', "-"))) name=(name) checked=(checked) class="size-4" />(label)
                    </label>
                }
            </div>
        </fieldset>
        <div class="flex flex-wrap gap-6">
            <label class="flex items-center gap-2 text-sm">
                <input type="checkbox" id=(id("active")) name="is_active" checked=(form.is_active.is_some()) class="size-4" />"Active"
            </label>
            if !creating {
                <label class="flex items-center gap-2 text-sm">
                    <input type="checkbox" id=(id("verified")) name="is_verified" checked=(form.is_verified.is_some()) class="size-4" />"Email verified"
                </label>
            }
        </div>
    })
}

/// The create-account page body.
#[component]
async fn create_form(
    /// Current values.
    form: UserForm,
    /// Messages to show.
    errors: Vec<String>,
) -> Result<impl View> {
    Ok(view! {
        <div class="mx-auto max-w-3xl space-y-6">
            <div class="flex items-end justify-between gap-4">
                <div>
                    <p class="text-xs font-medium tracking-wide text-muted-foreground uppercase">"Access control and accounts"</p>
                    <h1 class="text-2xl font-semibold">"New user"</h1>
                </div>
                <a href="/admin/users" class="text-sm">"Back to users"</a>
            </div>
            form_errors(errors: errors)
            <form id="admin-create-user-form" method="post" action="/admin/users" class="space-y-6 rounded-xl border border-border bg-card p-6">
                user_fields(form: form, prefix: "new-user".into(), role_prefix: "role".into(), creating: true)
                <button id="btn-create-user" type="submit" class="h-10 rounded-lg bg-primary px-5 text-sm font-medium text-primary-foreground">"Create user account"</button>
            </form>
        </div>
    })
}

/// The blank create form.
#[page("/admin/users/new")]
pub async fn new_user_page(cx: &Cx) -> Result<impl View> {
    admin_only(cx).await?;
    Ok(view! { create_form(form: UserForm::blank(), errors: Vec::new()) })
}

/// Creates an account. Accounts made here are verified; hosts get a
/// verified host profile and bookers an empty booker profile.
#[page(POST "/admin/users")]
pub async fn create_user(
    cx: &Cx,
    /// The create form.
    Form(form): Form<UserForm>,
) -> Result<impl View> {
    let admin = admin_only(cx).await?;
    let errors = match form.validate(true) {
        Err(errors) => errors,
        Ok(valid) => {
            let has = |role: &str| valid.roles.iter().any(|r| r == role);
            let request = NewUserRequest {
                email: valid.email.clone(),
                password: valid.password.clone().unwrap_or_default(),
                first_name: valid.first_name.clone(),
                last_name: valid.last_name.clone(),
                phone_number: valid.phone_number.clone(),
                is_active: valid.is_active,
                is_verified: true,
                attributes: Some(serde_json::Value::Object(valid.permissions.clone())),
                roles: Some(valid.roles.clone()),
                booker_profile: has("booker").then_some(NewBookerProfile {
                    emergency_contacts: None,
                    booking_preferences: None,
                    loyalty: None,
                }),
                host_profile: has("host").then(|| NewHostProfile {
                    verified_status: Some("verified".to_string()),
                    payout_details: None,
                    description: None,
                }),
                default_currency: Some(valid.default_currency.clone()),
            };
            match get_api_client(cx).create_user(&request).await {
                Ok(created) => {
                    tracing::info!(user_id = %created.id, admin_id = %admin.user_id, "user created");
                    return Err(see_other("/admin/users").into());
                }
                Err(e) => {
                    tracing::info!("user create rejected: {e}");
                    vec![if e.to_string().to_lowercase().contains("exist") {
                        "An account with that email already exists.".to_string()
                    } else {
                        "The account could not be created. Check the values and try again."
                            .to_string()
                    }]
                }
            }
        }
    };
    Ok(view! { create_form(form: form, errors: errors) })
}

/// The edit dialog fragment (swapped into `#user-dialog-slot`).
#[component]
async fn edit_dialog(
    /// The account being edited.
    user_id: Uuid,
    /// Current values.
    form: UserForm,
    /// Messages to show.
    errors: Vec<String>,
) -> Result<impl View> {
    let action = format!("/admin/users/{user_id}");
    let title = format!("Edit {} {}", form.first_name, form.last_name);
    Ok(view! {
        <dialog id="admin-user-edit-dialog" data-auto-open="" aria-labelledby="edit-user-title" class="m-auto max-h-[90vh] w-[min(720px,95vw)] overflow-y-auto rounded-xl bg-card p-6 text-card-foreground backdrop:bg-black/50">
            <form id="edit-user-form" method="post" action=(action.clone()) hx-post=(action) hx-target="#user-dialog-slot" hx-swap="innerHTML" class="space-y-6">
                <div class="flex items-start justify-between gap-4">
                    <div>
                        <h2 id="edit-user-title" class="text-xl font-semibold">(title)</h2>
                        <p class="font-mono text-xs text-muted-foreground">(user_id.to_string())</p>
                    </div>
                    <button type="button" data-dialog-close="" class="h-9 rounded-lg border border-border px-3 text-sm">"Close"</button>
                </div>
                form_errors(errors: errors)
                user_fields(form: form, prefix: "edit-user".into(), role_prefix: "edit-role".into(), creating: false)
                <button id="edit-user-save-btn" type="submit" class="h-10 rounded-lg bg-primary px-5 text-sm font-medium text-primary-foreground">"Save changes"</button>
            </form>
        </dialog>
    })
}

fn parse_id(cx: &Cx) -> Result<Uuid> {
    Uuid::from_str(path_param::<Id>(cx)).map_err(|_| not_found().into())
}

/// Opens the edit dialog for an account.
#[page("/admin/users/{id}/edit")]
pub async fn edit_user_dialog(cx: &Cx) -> Result<impl View> {
    admin_only(cx).await?;
    let id = parse_id(cx)?;
    let user = get_api_client(cx)
        .get_user_profile(id)
        .await
        .map_err(|_| not_found())?;
    Ok(view! { edit_dialog(user_id: id, form: UserForm::from_user(&user), errors: Vec::new()) })
}

/// Saves an account. Capability flags are merged into the stored
/// attributes so unrelated attributes survive. On success an HTMX request
/// gets `HX-Redirect`, a plain form post a 303.
#[page(POST "/admin/users/{id}")]
pub async fn update_user(
    cx: &Cx,
    /// The edit form.
    Form(form): Form<UserForm>,
) -> Result<impl View> {
    let admin = admin_only(cx).await?;
    let id = parse_id(cx)?;
    let api = get_api_client(cx);
    let stored = api.get_user_profile(id).await.map_err(|_| not_found())?;
    let errors = match form.validate(false) {
        Err(errors) => errors,
        Ok(valid) => {
            let mut attributes = stored.attributes.as_object().cloned().unwrap_or_default();
            [
                "can_manage_listings",
                "can_manage_bookings",
                "can_configure_rates",
                "can_manage_users",
            ]
            .iter()
            .for_each(|k| {
                attributes.remove(*k);
            });
            attributes.extend(valid.permissions.clone());
            let request = UpdateUserRequest {
                email: Some(valid.email.clone()),
                password: valid.password.clone(),
                first_name: Some(valid.first_name.clone()),
                last_name: Some(valid.last_name.clone()),
                phone_number: valid.phone_number.clone(),
                is_active: Some(valid.is_active),
                is_verified: Some(valid.is_verified),
                attributes: Some(serde_json::Value::Object(attributes)),
                roles: Some(valid.roles.clone()),
                booker_profile: None,
                host_profile: None,
                default_currency: Some(valid.default_currency.clone()),
                slug: valid.slug.clone(),
            };
            match api.update_user(id, &request).await {
                Ok(_) => {
                    tracing::info!(user_id = %id, admin_id = %admin.user_id, "user updated");
                    let target = "/admin/users?status=all&notice=updated";
                    if !topcoat::htmx::hx_request(cx) {
                        return Err(see_other(target).into());
                    }
                    response_headers(cx).append(
                        HeaderName::from_static("hx-redirect"),
                        HeaderValue::from_static(target),
                    );
                    Vec::new()
                }
                Err(e) if e.to_string().contains("already taken") => vec![format!(
                    "The profile URL “{}” is already taken.",
                    valid.slug.clone().unwrap_or_default()
                )],
                Err(e) => {
                    tracing::info!(user_id = %id, "user update rejected: {e}");
                    vec!["The account could not be saved. Administrator roles cannot be removed, and emails must be unique.".to_string()]
                }
            }
        }
    };
    Ok(view! { edit_dialog(user_id: id, form: form, errors: errors) })
}
