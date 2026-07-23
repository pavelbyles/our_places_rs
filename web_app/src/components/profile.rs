use crate::auth::UserProfile;
use leptos::prelude::*;
use leptos_meta::Title;

#[component]
pub fn ProfilePage() -> impl IntoView {
    let auth_context = use_context::<crate::app::AuthContext>().expect("AuthContext missing");

    let user_resource = auth_context.user;

    view! {
        <Title text="My Profile" />
        <div class="max-w-4xl mx-auto py-12 px-4 sm:px-6 lg:px-8">
            <h1 class="text-3xl font-extrabold tracking-tight text-base-content mb-8">
                "Profile Settings"
            </h1>

            <Suspense fallback=move || view! { <span class="loading loading-spinner text-primary"></span> }>
                {move || match user_resource.get() {
                    Some(Ok(Some(user))) => {
                        view! {
                            <div class="space-y-6">
                                <div class="bg-base-200 rounded-lg shadow p-6">
                                    <h2 class="text-xl font-bold mb-4">"General Settings"</h2>
                                    <ProfileForm user=user.clone() />
                                </div>
                                <div class="bg-base-200 rounded-lg shadow p-6">
                                    <h2 class="text-xl font-bold mb-4">"Security"</h2>
                                    <ChangePasswordSection email=user.email.clone() />
                                </div>
                                <div class="bg-base-200 rounded-lg shadow p-6">
                                    <h2 class="text-xl font-bold mb-4">"Email Settings"</h2>
                                    <ChangeEmailSection email=user.email.clone() />
                                </div>
                                <div class="bg-error/10 border border-error rounded-lg shadow p-6">
                                    <h2 class="text-xl font-bold text-error mb-4">"Danger Zone"</h2>
                                    <DeactivateAccountSection email=user.email.clone() />
                                </div>
                            </div>
                        }.into_any()
                    }
                    Some(Ok(None)) => view! {
                        <div class="alert alert-warning shadow-lg">
                            <span>"You must be logged in to view this page."</span>
                        </div>
                    }.into_any(),
                    Some(Err(_)) => view! {
                        <div class="alert alert-error shadow-lg">
                            <span>"Error loading profile"</span>
                        </div>
                    }.into_any(),
                    None => view! { <span></span> }.into_any(),
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn ProfileForm(user: UserProfile) -> impl IntoView {
    let (currency, set_currency) = signal(user.default_currency.clone());
    let (saving, set_saving) = signal(false);
    let (success_msg, set_success_msg) = signal(None::<String>);
    let (error_msg, set_error_msg) = signal(None::<String>);

    let auth_context = use_context::<crate::app::AuthContext>().expect("AuthContext missing");

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_saving.set(true);
        set_error_msg.set(None);
        set_success_msg.set(None);

        let curr = currency.get();
        let user_id = user.id.clone();
        let auth_context = auth_context.clone();

        leptos::task::spawn_local(async move {
            match update_user_currency(user_id, curr).await {
                Ok(_) => {
                    set_success_msg.set(Some("Preferences updated successfully.".to_string()));
                    auth_context.refresh();
                }
                Err(e) => {
                    set_error_msg.set(Some(e.to_string()));
                }
            }
            set_saving.set(false);
        });
    };

    let currencies = vec![
        ("USD", "US Dollar (USD)"),
        ("EUR", "Euro (EUR)"),
        ("GBP", "British Pound (GBP)"),
        ("JMD", "Jamaican Dollar (JMD)"),
        ("CAD", "Canadian Dollar (CAD)"),
    ];

    view! {
        <form on:submit=on_submit class="space-y-6 max-w-md">
            {move || success_msg.get().map(|msg| view! {
                <div class="alert alert-success shadow-lg">
                    <span>{msg}</span>
                </div>
            })}

            {move || error_msg.get().map(|msg| view! {
                <div class="alert alert-error shadow-lg">
                    <span>{msg}</span>
                </div>
            })}

            <div class="form-control">
                <label class="label">
                    <span class="label-text">"Name"</span>
                </label>
                <input type="text" class="input input-bordered" value=user.name.clone() disabled />
            </div>

            <div class="form-control">
                <label class="label">
                    <span class="label-text">"Email"</span>
                </label>
                <input type="text" class="input input-bordered" value=user.email.clone() disabled />
            </div>

            <div class="form-control">
                <label class="label">
                    <span class="label-text font-semibold">"Preferred Currency"</span>
                </label>
                <select
                    class="select select-bordered w-full"
                    on:change=move |ev| set_currency.set(event_target_value(&ev))
                >
                    {currencies.into_iter().map(|(code, name)| {
                        let is_selected = code == currency.get_untracked();
                        view! {
                            <option value=code selected=is_selected>{name}</option>
                        }
                    }).collect_view()}
                </select>
            </div>

            <div class="form-control mt-6">
                <button type="submit" class="btn btn-primary" disabled=move || saving.get()>
                    {move || if saving.get() { "Saving..." } else { "Save Changes" }}
                </button>
            </div>
        </form>
    }
}

#[server]
pub async fn update_user_currency(user_id: String, currency: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use web_app_common::api_client::{get_client, user_api_audience, user_api_url};

        let session = crate::auth::try_extract_session().await?;
        let app_session = crate::auth::AppSession::new(session);
        let validated = app_session
            .verify()
            .map_err(|_| ServerFnError::new("Unauthorized"))?;

        if validated.state.user_id != user_id {
            return Err(ServerFnError::new("Unauthorized"));
        }

        let url = format!("{}/api/v1/users/user/{}", user_api_url(), user_id);
        let audience = user_api_audience();

        let payload = serde_json::json!({
            "default_currency": currency
        });

        let response = get_client()
            .patch(&url, &audience, &payload)
            .await
            .map_err(|e| ServerFnError::new(format!("API Request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(ServerFnError::new("Failed to update user profile"));
        }

        validated
            .session
            .insert("user_default_currency", currency)
            .map_err(|_| ServerFnError::new("Failed to set session"))?;
    }

    Ok(())
}

#[component]
fn ChangePasswordSection(email: String) -> impl IntoView {
    let (current_password, set_current_password) = signal("".to_string());
    let (new_password, set_new_password) = signal("".to_string());
    let (code, set_code) = signal("".to_string());
    let (step, set_step) = signal(1); // 1: request, 2: confirm
    let (loading, set_loading) = signal(false);
    let (error_msg, set_error_msg) = signal(None::<String>);
    let (success_msg, set_success_msg) = signal(None::<String>);

    view! {
        <div class="max-w-md">
            {move || error_msg.get().map(|msg| view! {
                <div class="alert alert-error shadow-lg mb-4"><span>{msg}</span></div>
            })}
            {move || success_msg.get().map(|msg| view! {
                <div class="alert alert-success shadow-lg mb-4"><span>{msg}</span></div>
            })}

            {move || if step.get() == 1 {
                let email_clone = email.clone();
                let on_request = move |ev: leptos::ev::SubmitEvent| {
                    ev.prevent_default();
                    set_loading.set(true);
                    set_error_msg.set(None);
                    let current = current_password.get();
                    let email_req = email_clone.clone();

                    leptos::task::spawn_local(async move {
                        match crate::auth::request_password_change(email_req, current).await {
                            Ok(_) => {
                                set_step.set(2);
                                set_success_msg.set(Some("Verification code sent to your email.".to_string()));
                            }
                            Err(e) => set_error_msg.set(Some(e.to_string())),
                        }
                        set_loading.set(false);
                    });
                };

                view! {
                    <form on:submit=on_request class="space-y-4">
                        <div class="form-control">
                            <label class="label"><span class="label-text">"Current Password"</span></label>
                            <input type="password" required class="input input-bordered"
                                name="current_password" autocomplete="current-password"
                                prop:value=move || current_password.get()
                                on:input=move |ev| set_current_password.set(event_target_value(&ev)) />
                        </div>
                        <button type="submit" class="btn btn-primary" disabled=move || loading.get()>
                            {move || if loading.get() { "Requesting..." } else { "Change Password" }}
                        </button>
                    </form>
                }.into_any()
            } else {
                let email_clone = email.clone();
                let on_confirm = move |ev: leptos::ev::SubmitEvent| {
                    ev.prevent_default();
                    set_loading.set(true);
                    set_error_msg.set(None);

                    let otp = code.get();
                    let new_pass = new_password.get();
                    let email_conf = email_clone.clone();

                    leptos::task::spawn_local(async move {
                        match crate::auth::confirm_password_change(email_conf, otp, new_pass).await {
                            Ok(_) => {
                                // Will redirect to login since session is purged
                            }
                            Err(e) => set_error_msg.set(Some(e.to_string())),
                        }
                        set_loading.set(false);
                    });
                };

                view! {
                    <form on:submit=on_confirm class="space-y-4">
                        <div class="form-control">
                            <label class="label"><span class="label-text">"Verification Code"</span></label>
                            <input type="text" required class="input input-bordered"
                                name="verification_code" autocomplete="one-time-code"
                                prop:value=move || code.get()
                                on:input=move |ev| set_code.set(event_target_value(&ev)) />
                        </div>
                        <div class="form-control">
                            <label class="label"><span class="label-text">"New Password"</span></label>
                            <input type="password" required class="input input-bordered"
                                name="new_password" autocomplete="new-password"
                                prop:value=move || new_password.get()
                                on:input=move |ev| set_new_password.set(event_target_value(&ev)) />
                        </div>
                        <button type="submit" class="btn btn-primary" disabled=move || loading.get()>
                            {move || if loading.get() { "Confirming..." } else { "Confirm Password Change" }}
                        </button>
                    </form>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn ChangeEmailSection(email: String) -> impl IntoView {
    let (current_password, set_current_password) = signal("".to_string());
    let (new_email, set_new_email) = signal("".to_string());
    let (loading, set_loading) = signal(false);
    let (error_msg, set_error_msg) = signal(None::<String>);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error_msg.set(None);

        let current = current_password.get();
        let target_email = new_email.get();
        let email_clone = email.clone();

        leptos::task::spawn_local(async move {
            match crate::auth::change_email(email_clone, current, target_email).await {
                Ok(_) => {
                    // Redirects to /verify
                }
                Err(e) => set_error_msg.set(Some(e.to_string())),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="max-w-md">
            {move || error_msg.get().map(|msg| view! {
                <div class="alert alert-error shadow-lg mb-4"><span>{msg}</span></div>
            })}
            <form on:submit=on_submit class="space-y-4">
                <div class="form-control">
                    <label class="label"><span class="label-text">"Current Password"</span></label>
                    <input type="password" required class="input input-bordered"
                        prop:value=move || current_password.get()
                        on:input=move |ev| set_current_password.set(event_target_value(&ev)) />
                </div>
                <div class="form-control">
                    <label class="label"><span class="label-text">"New Email Address"</span></label>
                    <input type="email" required class="input input-bordered"
                        prop:value=move || new_email.get()
                        on:input=move |ev| set_new_email.set(event_target_value(&ev)) />
                </div>
                <button type="submit" class="btn btn-primary" disabled=move || loading.get()>
                    {move || if loading.get() { "Saving..." } else { "Change Email" }}
                </button>
            </form>
        </div>
    }
}

#[component]
fn DeactivateAccountSection(email: String) -> impl IntoView {
    let (current_password, set_current_password) = signal("".to_string());
    let (loading, set_loading) = signal(false);
    let (error_msg, set_error_msg) = signal(None::<String>);
    let (show_confirm, set_show_confirm) = signal(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if !show_confirm.get() {
            set_show_confirm.set(true);
            return;
        }

        set_loading.set(true);
        set_error_msg.set(None);
        let current = current_password.get();
        let email_clone = email.clone();

        leptos::task::spawn_local(async move {
            match crate::auth::deactivate_account(email_clone, current).await {
                Ok(_) => {
                    // Redirects to /
                }
                Err(e) => set_error_msg.set(Some(e.to_string())),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="max-w-md">
            <p class="text-sm mb-4">"Deactivating your account will hide your profile and all your active listings. This action cannot be undone from the dashboard."</p>
            {move || error_msg.get().map(|msg| view! {
                <div class="alert alert-error shadow-lg mb-4"><span>{msg}</span></div>
            })}
            <form on:submit=on_submit class="space-y-4">
                <div class="form-control">
                    <label class="label"><span class="label-text">"Current Password"</span></label>
                    <input type="password" required class="input input-bordered input-error"
                        prop:value=move || current_password.get()
                        on:input=move |ev| set_current_password.set(event_target_value(&ev)) />
                </div>
                <button type="submit" class="btn btn-error" disabled=move || loading.get()>
                    {move || if loading.get() {
                        "Processing..."
                    } else if show_confirm.get() {
                        "Are you sure? Click again to deactivate."
                    } else {
                        "Deactivate Account"
                    }}
                </button>
            </form>
        </div>
    }
}
