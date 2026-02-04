#[cfg(feature = "ssr")]
use common::models::NewUserRequest;
use leptos::form::ActionForm;
use leptos::prelude::*;

#[server]
pub async fn create_user_server(
    email: String,
    password: String,
    first_name: String,
    last_name: String,
    phone_number: Option<String>,
    #[server(default)] is_active: bool,
) -> Result<(), ServerFnError> {
    let request = NewUserRequest {
        email,
        password,
        first_name,
        last_name,
        phone_number,
        is_active,
        attributes: None,
        roles: None,
        booker_profile: None,
        host_profile: None,
    };

    let client = reqwest::Client::new();
    let res = client
        .post("http://localhost:8083/api/v1/users/")
        .json(&request)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if res.status().is_success() {
        Ok(())
    } else {
        Err(ServerFnError::new(format!(
            "Failed to create user: {}",
            res.status()
        )))
    }
}

#[component]
pub fn AdminPage() -> impl IntoView {
    let create_user = ServerAction::<CreateUserServer>::new();
    let value = create_user.value();

    view! {
        <h1>"Admin Page"</h1>
        <div class="tabs tabs-lift">
            <input type="radio" name="my_tabs_3" class="tab" aria-label="Add User" checked="checked" />
            <div class="tab-content bg-base-100 border-base-300 p-6">
                <ActionForm action=create_user attr:class="form-control w-full max-w-xs space-y-4">
                    <div>
                        <label class="label">
                            <span class="label-text">"Email"</span>
                        </label>
                        <input type="email" name="email" placeholder="Email" class="input input-bordered w-full max-w-xs" required />
                    </div>

                    <div>
                        <label class="label">
                            <span class="label-text">"Password"</span>
                        </label>
                        <input type="password" name="password" placeholder="Password" class="input input-bordered w-full max-w-xs" required />
                    </div>

                    <div>
                        <label class="label">
                            <span class="label-text">"First Name"</span>
                        </label>
                        <input type="text" name="first_name" placeholder="First Name" class="input input-bordered w-full max-w-xs" required />
                    </div>

                    <div>
                        <label class="label">
                            <span class="label-text">"Last Name"</span>
                        </label>
                        <input type="text" name="last_name" placeholder="Last Name" class="input input-bordered w-full max-w-xs" required />
                    </div>

                    <div>
                        <label class="label">
                            <span class="label-text">"Phone Number"</span>
                        </label>
                        <input type="tel" name="phone_number" placeholder="Phone Number" class="input input-bordered w-full max-w-xs" />
                    </div>

                    <div class="form-control">
                        <label class="label cursor-pointer">
                            <span class="label-text">"Is Active"</span>
                            <input type="checkbox" name="is_active" class="checkbox" checked="checked" value="true" />
                        </label>
                    </div>

                    <button type="submit" class="btn btn-primary" disabled=move || create_user.pending().get()>
                        {move || if create_user.pending().get() { "Creating..." } else { "Create User" }}
                    </button>

                    {move || value.get().map(|v| match v {
                        Ok(_) => view! { <div class="alert alert-success mt-4"><span>"User created successfully"</span></div> }.into_any(),
                        Err(e) => view! { <div class="alert alert-error mt-4"><span>{e.to_string()}</span></div> }.into_any(),
                    })}
                </ActionForm>
            </div>

            <input type="radio" name="my_tabs_3" class="tab" aria-label="Manage Users" />
            <div class="tab-content bg-base-100 border-base-300 p-6">"Manage Users Content (Placeholder)"</div>

            <input type="radio" name="my_tabs_3" class="tab" aria-label="Tab 3" />
            <div class="tab-content bg-base-100 border-base-300 p-6">Tab content 3</div>
        </div>
    }
}
