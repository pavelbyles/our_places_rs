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
                            <div class="bg-base-200 rounded-lg shadow p-6">
                                <ProfileForm user=user />
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
        let session_user_id = session.get::<String>("user_id").ok().flatten();

        if session_user_id.as_deref() != Some(user_id.as_str()) {
            return Err(ServerFnError::new("Unauthorized"));
        }

        let url = format!("{}/api/v1/users/{}", user_api_url(), user_id);
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

        session
            .insert("user_default_currency", currency)
            .map_err(|_| ServerFnError::new("Failed to set session"))?;
    }

    Ok(())
}
