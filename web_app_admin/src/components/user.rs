use crate::components::protected::RequireAuth;
#[cfg(feature = "ssr")]
use common::models::{NewUserRequest, UpdateUserRequest};
use leptos::form::ActionForm;
use leptos::html::Input;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateUserParams {
    pub email: String,
    pub password: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub can_manage_bookings: bool,
    #[serde(default)]
    pub can_manage_listings: bool,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub is_booker: bool,
    #[serde(default)]
    pub is_host: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpdateUserParams {
    pub id: String,
    pub email: String,
    pub password: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub can_manage_bookings: bool,
    #[serde(default)]
    pub can_manage_listings: bool,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub is_booker: bool,
    #[serde(default)]
    pub is_host: bool,
}

#[cfg(feature = "ssr")]
async fn ensure_admin() -> Result<(), ServerFnError> {
    let session = leptos_actix::extract::<actix_session::Session>()
        .await
        .map_err(|_| ServerFnError::new("Session not found"))?;
    let is_admin = session
        .get::<bool>("is_admin")
        .unwrap_or(None)
        .unwrap_or(false);
    if !is_admin {
        return Err(ServerFnError::new("Unauthorized: Admin access required"));
    }
    Ok(())
}

#[server]
pub async fn create_user_server(params: CreateUserParams) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    ensure_admin().await?;

    let mut roles = Vec::new();
    if params.is_booker {
        roles.push("booker".to_string());
    }
    if params.is_host {
        roles.push("host".to_string());
    }
    if params.is_admin {
        roles.push("admin".to_string());
    }

    let attributes = serde_json::json!({
        "can_manage_bookings": params.can_manage_bookings,
        "can_manage_listings": params.can_manage_listings,
    });

    let booker_profile = if params.is_booker {
        Some(common::models::NewBookerProfile {
            emergency_contacts: None,
            booking_preferences: None,
            loyalty: None,
        })
    } else {
        None
    };

    let host_profile = if params.is_host {
        Some(common::models::NewHostProfile {
            verified_status: Some("verified".to_string()),
            payout_details: None,
            description: None,
        })
    } else {
        None
    };

    let request = NewUserRequest {
        email: params.email,
        password: params.password,
        first_name: params.first_name,
        last_name: params.last_name,
        phone_number: params.phone_number,
        is_active: params.is_active,
        attributes: Some(attributes),
        roles: Some(roles),
        is_verified: true,
        booker_profile,
        host_profile,
        default_currency: None,
    };

    let api_url = crate::api_client::user_api_url();
    let res = crate::api_client::get_client()
        .post(&format!("{}/api/v1/users/", api_url), &api_url, &request)
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

#[server]
pub async fn update_user_server(params: UpdateUserParams) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    ensure_admin().await?;

    let mut roles = Vec::new();
    if params.is_booker {
        roles.push("booker".to_string());
    }
    if params.is_host {
        roles.push("host".to_string());
    }
    if params.is_admin {
        roles.push("admin".to_string());
    }

    let attributes = serde_json::json!({
        "can_manage_bookings": params.can_manage_bookings,
        "can_manage_listings": params.can_manage_listings,
    });

    let booker_profile = if params.is_booker {
        Some(common::models::NewBookerProfile {
            emergency_contacts: None,
            booking_preferences: None,
            loyalty: None,
        })
    } else {
        None
    };

    let host_profile = if params.is_host {
        Some(common::models::NewHostProfile {
            verified_status: Some("verified".to_string()),
            payout_details: None,
            description: None,
        })
    } else {
        None
    };

    let request = UpdateUserRequest {
        email: Some(params.email),
        password: Some(params.password),
        first_name: Some(params.first_name),
        last_name: Some(params.last_name),
        phone_number: params.phone_number,
        is_active: Some(params.is_active),
        attributes: Some(attributes),
        roles: Some(roles),
        is_verified: Some(true),
        booker_profile,
        host_profile,
        default_currency: None,
    };

    let api_url = crate::api_client::user_api_url();
    let res = crate::api_client::get_client()
        .patch(
            &format!("{}/api/v1/users/user/{}", api_url, params.id),
            &api_url,
            &request,
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if res.status().is_success() {
        Ok(())
    } else {
        Err(ServerFnError::new(format!(
            "Failed to update user: {}",
            res.status()
        )))
    }
}

#[server]
pub async fn get_users_server(
    search: Option<String>,
    is_deleted: Option<bool>,
) -> Result<Vec<common::models::UserResponse>, ServerFnError> {
    #[cfg(feature = "ssr")]
    ensure_admin().await?;

    let api_url = crate::api_client::user_api_url();
    let mut url = format!("{}/api/v1/users/?page=1&per_page=50", api_url);

    if let Some(s) = search {
        url.push_str(&format!("&search={}", s));
    }
    if let Some(deleted) = is_deleted {
        url.push_str(&format!("&is_deleted={}", deleted));
    }

    let res = crate::api_client::get_client()
        .get(&url, &api_url)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !res.status().is_success() {
        return Err(ServerFnError::new(format!(
            "Failed to fetch users: {}",
            res.status()
        )));
    }

    let users: Vec<common::models::UserResponse> = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(users)
}

#[server]
pub async fn soft_delete_user_server(id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    ensure_admin().await?;

    let api_url = crate::api_client::user_api_url();
    let url = format!("{}/api/v1/users/user/{}", api_url, id);

    let res = crate::api_client::get_client()
        .delete(&url, &api_url)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if res.status().is_success() {
        Ok(())
    } else {
        Err(ServerFnError::new(format!(
            "Failed to delete user: {}",
            res.status()
        )))
    }
}

#[server]
pub async fn restore_user_server(id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    ensure_admin().await?;

    let api_url = crate::api_client::user_api_url();
    let url = format!("{}/api/v1/users/user/{}/restore", api_url, id);

    let res = crate::api_client::get_client()
        .post(&url, &api_url, &serde_json::json!({}))
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if res.status().is_success() {
        Ok(())
    } else {
        Err(ServerFnError::new(format!(
            "Failed to restore user: {}",
            res.status()
        )))
    }
}

#[server]
pub async fn hard_delete_user_server(id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    ensure_admin().await?;

    let api_url = crate::api_client::user_api_url();
    let url = format!("{}/api/v1/users/user/{}/hard", api_url, id);

    let res = crate::api_client::get_client()
        .delete(&url, &api_url)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if res.status().is_success() {
        Ok(())
    } else {
        Err(ServerFnError::new(format!(
            "Failed to hard delete user: {}",
            res.status()
        )))
    }
}

#[server]
pub async fn get_user_server(email: String) -> Result<common::models::UserResponse, ServerFnError> {
    let api_url = crate::api_client::user_api_url();
    let url = format!("{}/api/v1/users/user/{}", api_url, email);

    let res = crate::api_client::get_client()
        .get(&url, &api_url)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !res.status().is_success() {
        return Err(ServerFnError::new(format!(
            "Failed to fetch user: {}",
            res.status()
        )));
    }

    let user: common::models::UserResponse = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(user)
}

#[component]
pub fn UserPage() -> impl IntoView {
    let create_user = ServerAction::<CreateUserServer>::new();
    let update_user = ServerAction::<UpdateUserServer>::new();
    let soft_delete_user_action = ServerAction::<SoftDeleteUserServer>::new();
    let restore_user_action = ServerAction::<RestoreUserServer>::new();
    let hard_delete_user_action = ServerAction::<HardDeleteUserServer>::new();
    let created_user_value = create_user.value();
    let updated_user_value = update_user.value();

    let email_ref = NodeRef::<Input>::new();
    let first_name_ref = NodeRef::<Input>::new();
    let last_name_ref = NodeRef::<Input>::new();
    let id_ref = NodeRef::<Input>::new();
    let phone_number_ref = NodeRef::<Input>::new();
    let is_active_ref = NodeRef::<Input>::new();
    let can_manage_bookings_ref = NodeRef::<Input>::new();
    let can_manage_listings_ref = NodeRef::<Input>::new();
    let is_admin_ref = NodeRef::<Input>::new();
    let is_booker_ref = NodeRef::<Input>::new();
    let is_host_ref = NodeRef::<Input>::new();

    let (email, set_email) = signal(String::new());
    let (first_name, set_first_name) = signal(String::new());
    let (last_name, set_last_name) = signal(String::new());

    // User List State (Active)
    let (search, set_search) = signal(String::new());
    let users_resource = Resource::new(
        move || {
            (
                search.get(),
                soft_delete_user_action.version().get(),
                restore_user_action.version().get(),
                hard_delete_user_action.version().get(),
                created_user_value.get(),
                updated_user_value.get(),
            )
        },
        |(s, _, _, _, _, _)| async move { get_users_server(Some(s), Some(false)).await },
    );

    // Deleted Users State
    let (deleted_search, set_deleted_search) = signal(String::new());
    let deleted_users_resource = Resource::new(
        move || {
            (
                deleted_search.get(),
                soft_delete_user_action.version().get(),
                restore_user_action.version().get(),
                hard_delete_user_action.version().get(),
                created_user_value.get(),
                updated_user_value.get(),
            )
        },
        |(s, _, _, _, _, _)| async move { get_users_server(Some(s), Some(true)).await },
    );

    let session_user_resource = Resource::new(
        || (),
        |_| async move { crate::auth::get_current_session_user().await },
    );

    let is_form_valid = move || {
        let e = email.get();
        // Basic check: must contain '@' and domain must contain '.'
        let email_format_valid = e.contains('@')
            && e.split('@')
                .nth(1)
                .is_some_and(|domain| domain.contains('.'));
        !e.is_empty()
            && email_format_valid
            && !first_name.get().is_empty()
            && !last_name.get().is_empty()
    };

    view! {
        <RequireAuth>
            {move || {
                match session_user_resource.get() {
                    Some(Ok(Some(user))) if user.is_admin => {
                        view! {
                            <h1>"User Admin Page"</h1>
                            <div class="tabs tabs-lift">
                                <input type="radio" name="my_tabs_3" class="tab" aria-label="Add User" checked="checked" />
                                <div class="tab-content bg-base-100 border-base-300 p-6">
                    <ActionForm action=create_user attr:class="form-control w-full max-w-xs space-y-4">
                        <hidden-input name="params[id]" value="" />
                        <div>
                        <label class="label">
                            <span class="label-text">"Email"</span>
                        </label>
                        <input type="email" name="params[email]" placeholder="Email" class="input input-bordered w-full max-w-xs" required />
                    </div>

                    <div>
                        <label class="label">
                            <span class="label-text">"Password"</span>
                        </label>
                        <input type="password" name="params[password]" placeholder="Password" class="input input-bordered w-full max-w-xs" required />
                    </div>

                    <div>
                        <label class="label">
                            <span class="label-text">"First Name"</span>
                        </label>
                        <input type="text" name="params[first_name]" placeholder="First Name" class="input input-bordered w-full max-w-xs" required />
                    </div>

                    <div>
                        <label class="label">
                            <span class="label-text">"Last Name"</span>
                        </label>
                        <input type="text" name="params[last_name]" placeholder="Last Name" class="input input-bordered w-full max-w-xs" required />
                    </div>

                    <div>
                        <label class="label">
                            <span class="label-text">"Phone Number"</span>
                        </label>
                        <input type="tel" name="params[phone_number]" placeholder="Phone Number" class="input input-bordered w-full max-w-xs" />
                    </div>

                    <div class="form-control">
                        <label class="label cursor-pointer">
                            <span class="label-text">"Is Active"</span>
                            <input type="checkbox" name="params[is_active]" class="checkbox" checked="checked" value="true" />
                        </label>
                    </div>

                    <div class="divider">Attributes</div>
                    <div class="overflow-x-auto">
                        <table class="table table-xs w-full">
                            <thead>
                                <tr>
                                    <th class="text-left w-full">"Attribute"</th>
                                    <th class="text-left">"Value"</th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr>
                                    <td class="text-left font-normal">"Can Manage Bookings"</td>
                                    <td><div class="flex justify-end w-full"><input type="checkbox" name="params[can_manage_bookings]" class="checkbox checkbox-sm" value="true" /></div></td>
                                </tr>
                                <tr>
                                    <td class="text-left font-normal">"Can Manage Listings"</td>
                                    <td><div class="flex justify-end w-full"><input type="checkbox" name="params[can_manage_listings]" class="checkbox checkbox-sm" value="true" /></div></td>
                                </tr>
                            </tbody>
                        </table>
                    </div>

                    <div class="divider">Roles</div>
                    <div class="overflow-x-auto">
                        <table class="table table-xs w-full">
                            <thead>
                                <tr>
                                    <th class="text-left w-full">"Role"</th>
                                    <th class="text-left">"Value"</th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr>
                                    <td class="text-left font-normal">"Is Booker"</td>
                                    <td><div class="flex justify-end w-full"><input type="checkbox" name="params[is_booker]" class="checkbox checkbox-sm" value="true" /></div></td>
                                </tr>
                                <tr>
                                    <td class="text-left font-normal">"Is Host"</td>
                                    <td><div class="flex justify-end w-full"><input type="checkbox" name="params[is_host]" class="checkbox checkbox-sm" value="true" /></div></td>
                                </tr>
                                <tr>
                                    <td class="text-left font-normal">"Is Admin"</td>
                                    <td><div class="flex justify-end w-full"><input type="checkbox" name="params[is_admin]" class="checkbox checkbox-sm" value="true" /></div></td>
                                </tr>
                            </tbody>
                        </table>
                    </div>

                    <button type="submit" class="btn btn-primary" disabled=move || create_user.pending().get()>
                        {move || if create_user.pending().get() { "Creating..." } else { "Create User" }}
                    </button>

                    {move || created_user_value.get().map(|v| match v {
                        Ok(_) => view! { <div class="alert alert-success mt-4"><span>"User created successfully"</span></div> }.into_any(),
                        Err(e) => view! { <div class="alert alert-error mt-4"><span>{e.to_string()}</span></div> }.into_any(),
                    })}
                </ActionForm>
            </div>

            <input type="radio" name="my_tabs_3" class="tab" aria-label="Manage Users" />
            <div class="tab-content bg-base-100 border-base-300 p-6">
                <div class="flex w-full flex-col lg:flex-row">
                    <div class="card bg-base-300 rounded-box grid grow place-items-start">
                        <ActionForm action=update_user attr:class="form-control w-full max-w-xs space-y-4">
                            <input type="hidden" name="params[id]" node_ref=id_ref />
                            <div>
                                <label class="label">
                                    <span class="label-text">"Email"</span>
                                </label>
                                <input type="email" name="params[email]" placeholder="Email" class="input input-bordered w-full max-w-xs" required
                                    node_ref=email_ref
                                    on:input=move |_| set_email.set(email_ref.get().expect("input").value())
                                />
                            </div>

                            <div>
                                <label class="input">
                                    <input type="password" name="params[password]" placeholder="Password" class="input input-bordered w-full max-w-xs" />
                                    <span class="badge badge-neutral badge-xs">Optional</span>
                                </label>
                            </div>

                            <div>
                                <label class="label">
                                    <span class="label-text">"First Name"</span>
                                </label>
                                <input type="text" name="params[first_name]" placeholder="First Name" class="input input-bordered w-full max-w-xs" required
                                    node_ref=first_name_ref
                                    on:input=move |_| set_first_name.set(first_name_ref.get().expect("input").value())
                                />
                            </div>

                            <div>
                                <label class="label">
                                    <span class="label-text">"Last Name"</span>
                                </label>
                                <input type="text" name="params[last_name]" placeholder="Last Name" class="input input-bordered w-full max-w-xs" required
                                    node_ref=last_name_ref
                                    on:input=move |_| set_last_name.set(last_name_ref.get().expect("input").value())
                                />
                            </div>

                            <div>
                                <label class="label">
                                    <span class="label-text">"Phone Number"</span>
                                </label>
                                <input type="tel" name="params[phone_number]" placeholder="Phone Number" class="input input-bordered w-full max-w-xs" node_ref=phone_number_ref />
                            </div>

                            <div class="form-control">
                                <label class="label cursor-pointer">
                                    <span class="label-text">"Is Active"</span>
                                    <input type="checkbox" name="params[is_active]" class="checkbox" checked="checked" value="true" node_ref=is_active_ref />
                                </label>
                            </div>

                            <div class="divider">Attributes</div>
                            <div class="overflow-x-auto">
                                <table class="table table-xs w-full">
                                    <thead>
                                        <tr>
                                            <th class="text-left w-full">"Attribute"</th>
                                            <th class="text-left">"Value"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <tr>
                                            <td class="text-left font-normal">"Can Manage Bookings"</td>
                                            <td><div class="flex justify-end w-full"><input type="checkbox" name="params[can_manage_bookings]" class="checkbox checkbox-sm" value="true" node_ref=can_manage_bookings_ref /></div></td>
                                        </tr>
                                        <tr>
                                            <td class="text-left font-normal">"Can Manage Listings"</td>
                                            <td><div class="flex justify-end w-full"><input type="checkbox" name="params[can_manage_listings]" class="checkbox checkbox-sm" value="true" node_ref=can_manage_listings_ref /></div></td>
                                        </tr>
                                    </tbody>
                                </table>
                            </div>

                            <div class="divider">Roles</div>
                            <div class="overflow-x-auto">
                                <table class="table table-xs w-full">
                                    <thead>
                                        <tr>
                                            <th class="text-left w-full">"Role"</th>
                                            <th class="text-left">"Value"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <tr>
                                            <td class="text-left font-normal">"Is Booker"</td>
                                            <td><div class="flex justify-end w-full"><input type="checkbox" name="params[is_booker]" class="checkbox checkbox-sm" value="true" node_ref=is_booker_ref /></div></td>
                                        </tr>
                                        <tr>
                                            <td class="text-left font-normal">"Is Host"</td>
                                            <td><div class="flex justify-end w-full"><input type="checkbox" name="params[is_host]" class="checkbox checkbox-sm" value="true" node_ref=is_host_ref /></div></td>
                                        </tr>
                                        <tr>
                                            <td class="text-left font-normal">"Is Admin"</td>
                                            <td><div class="flex justify-end w-full"><input type="checkbox" name="params[is_admin]" class="checkbox checkbox-sm" value="true" node_ref=is_admin_ref /></div></td>
                                        </tr>
                                    </tbody>
                                </table>
                            </div>

                            <button type="submit" class="btn btn-primary" disabled=move || update_user.pending().get() || !is_form_valid()>
                                {move || if update_user.pending().get() { "Updating..." } else { "Update User" }}
                            </button>

                            {move || updated_user_value.get().map(|v| match v {
                                Ok(_) => view! { <div class="alert alert-success mt-4"><span>"User updated successfully"</span></div> }.into_any(),
                                Err(e) => view! { <div class="alert alert-error mt-4"><span>{e.to_string()}</span></div> }.into_any(),
                            })}
                        </ActionForm>

                    </div>
                    <div class="divider lg:divider-horizontal">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="size-6">
                            <path stroke-linecap="round" stroke-linejoin="round" d="m18.75 4.5-7.5 7.5 7.5 7.5m-6-15L5.25 12l7.5 7.5" />
                        </svg>
                    </div>
                    <div class="card bg-base-300 rounded-box grow p-2">
                        <div class="w-full" style="height: 24rem; overflow-y: auto;">
                            <h3>User List</h3>
                            <label class="input">
                                <svg class="h-[1em] opacity-50" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                                    <g
                                    stroke-linejoin="round"
                                    stroke-linecap="round"
                                    stroke-width="2.5"
                                    fill="none"
                                    stroke="currentColor"
                                    >
                                    <circle cx="11" cy="11" r="8"></circle>
                                    <path d="m21 21-4.3-4.3"></path>
                                    </g>
                                </svg>
                                <input
                                    type="search"
                                    class="grow"
                                    placeholder="Search"
                                    prop:value=move || search.get()
                                    on:input=move |ev| set_search.set(event_target_value(&ev))
                                />
                            </label>
                            <ul class="list bg-base-100 rounded-box shadow-md">
                                <li class="p-4 pb-2 text-xs opacity-60 tracking-wide">"Select a user to edit:"</li>

                                <Suspense fallback=move || view! { <li class="p-4">"Loading..."</li> }>
                                    {move || {
                                        users_resource.get().map(|result| match result {
                                            Ok(users) => view! {
                                                <For
                                                    each=move || users.clone()
                                                    key=|u| u.id
                                                    children=move |user: common::models::UserResponse| {
                                                        let user_clone = user.clone();
                                                        let user_clone2 = user.clone();
                                                        view! {
                                                            <li class="list-row hover:bg-base-200 cursor-pointer"
                                                                on:click=move |_| {
                                                                    let email_to_fetch = user_clone.email.clone();
                                                                    spawn_local(async move {
                                                                        match get_user_server(email_to_fetch).await {
                                                                            Ok(fresh_user) => {
                                                                                 set_email.set(fresh_user.email.clone());
                                                                                 set_first_name.set(fresh_user.first_name.clone());
                                                                                 set_last_name.set(fresh_user.last_name.clone());

                                                                                 if let Some(ref input) = id_ref.get() {
                                                                                     input.set_value(&fresh_user.id.to_string());
                                                                                 }
                                                                                 if let Some(ref input) = email_ref.get() {
                                                                                     input.set_value(&fresh_user.email);
                                                                                 }
                                                                                 if let Some(ref input) = first_name_ref.get() {
                                                                                     input.set_value(&fresh_user.first_name);
                                                                                 }
                                                                                 if let Some(ref input) = last_name_ref.get() {
                                                                                     input.set_value(&fresh_user.last_name);
                                                                                 }
                                                                                 if let Some(ref input) = phone_number_ref.get() {
                                                                                     input.set_value(&fresh_user.phone_number.clone().unwrap_or_default());
                                                                                 }
                                                                                 if let Some(ref input) = is_active_ref.get() {
                                                                                     input.set_checked(fresh_user.is_active);
                                                                                 }

                                                                                 // Populate Attributes
                                                                                 let attrs = &fresh_user.attributes;
                                                                                 if let Some(ref input) = can_manage_bookings_ref.get() {
                                                                                     input.set_checked(attrs.get("can_manage_bookings").and_then(|v| v.as_bool()).unwrap_or(false));
                                                                                 }
                                                                                 if let Some(ref input) = can_manage_listings_ref.get() {
                                                                                     input.set_checked(attrs.get("can_manage_listings").and_then(|v| v.as_bool()).unwrap_or(false));
                                                                                 }

                                                                                 // Populate Roles
                                                                                 let roles = &fresh_user.roles;
                                                                                 if let Some(ref input) = is_booker_ref.get() {
                                                                                     input.set_checked(roles.contains(&"booker".to_string()));
                                                                                 }
                                                                                 if let Some(ref input) = is_host_ref.get() {
                                                                                     input.set_checked(roles.contains(&"host".to_string()));
                                                                                 }
                                                                                 if let Some(ref input) = is_admin_ref.get() {
                                                                                     input.set_checked(roles.iter().any(|r| r.to_lowercase() == "admin"));
                                                                                 }
                                                                            },
                                                                            Err(e) => tracing::error!("Failed to fetch user details: {}", e),
                                                                        }
                                                                    });
                                                                }
                                                            >
                                                                <div><img class="size-10 rounded-box" src="https://img.daisyui.com/images/profile/demo/1@94.webp"/></div>
                                                                <div>
                                                                    <div>{format!("{} {}", user_clone2.first_name, user_clone2.last_name)}</div>
                                                                    <div class="text-xs uppercase font-semibold opacity-60">{user_clone2.email}</div>
                                                                </div>
                                                                <div class="flex gap-1 ml-auto">
                                                                    <button class="btn btn-square btn-ghost" title="Edit User">
                                                                         <svg class="size-[1.2em]" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g stroke-linejoin="round" stroke-linecap="round" stroke-width="2" fill="none" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" d="m16.862 4.487 1.687-1.688a1.875 1.875 0 1 1 2.652 2.652L6.832 19.82a4.5 4.5 0 0 1-1.897 1.13l-2.685.8.8-2.685a4.5 4.5 0 0 1 1.13-1.897L16.863 4.487Zm0 0L19.5 7.125" /></g></svg>
                                                                    </button>
                                                                    <ActionForm action=soft_delete_user_action attr:class="inline">
                                                                        <input type="hidden" name="id" value=user_clone2.id.to_string() />
                                                                        <button type="submit" class="btn btn-square btn-ghost text-error" title="Delete User">
                                                                            <svg class="size-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                                                            </svg>
                                                                        </button>
                                                                    </ActionForm>
                                                                </div>
                                                            </li>
                                                        }
                                                    }
                                                />
                                            }.into_any(),
                                            Err(e) => view! { <li class="text-error p-4">{format!("Error: {}", e)}</li> }.into_any()
                                        })
                                    }}
                                </Suspense>
                            </ul>
                        </div>
                    </div>
                </div>
            </div>

            <input type="radio" name="my_tabs_3" class="tab" aria-label="Deleted Users" />
            <div class="tab-content bg-base-100 border-base-300 p-6">
                <div class="card bg-base-300 rounded-box p-4">
                    <div class="flex justify-between items-center mb-4">
                        <h3 class="font-bold text-lg">"Deleted Users Archive"</h3>
                        <label class="input input-sm">
                            <input
                                type="search"
                                class="grow"
                                placeholder="Search deleted users..."
                                prop:value=move || deleted_search.get()
                                on:input=move |ev| set_deleted_search.set(event_target_value(&ev))
                            />
                        </label>
                    </div>
                    <ul class="list bg-base-100 rounded-box shadow-md">
                        <Suspense fallback=move || view! { <li class="p-4">"Loading deleted users..."</li> }>
                            {move || {
                                deleted_users_resource.get().map(|result| match result {
                                    Ok(users) if users.is_empty() => view! { <li class="p-4 text-center opacity-60">"No soft-deleted users found"</li> }.into_any(),
                                    Ok(users) => view! {
                                        <For
                                            each=move || users.clone()
                                            key=|u| u.id
                                            children=move |user: common::models::UserResponse| {
                                                let id_str1 = user.id.to_string();
                                                let id_str2 = user.id.to_string();
                                                view! {
                                                    <li class="list-row flex justify-between items-center p-3 hover:bg-base-200">
                                                        <div>
                                                            <div class="font-semibold">{format!("{} {}", user.first_name, user.last_name)}</div>
                                                            <div class="text-xs opacity-60">{user.email}</div>
                                                            {user.deleted_at.map(|d| view! { <div class="text-xs text-error">"Deleted: " {d.format("%Y-%m-%d %H:%M").to_string()}</div> })}
                                                        </div>
                                                        <div class="flex gap-2">
                                                            <ActionForm action=restore_user_action attr:class="inline">
                                                                <input type="hidden" name="id" value=id_str1 />
                                                                <button type="submit" class="btn btn-sm btn-success">
                                                                    "Restore"
                                                                </button>
                                                            </ActionForm>
                                                            <ActionForm action=hard_delete_user_action attr:class="inline">
                                                                <input type="hidden" name="id" value=id_str2 />
                                                                <button type="submit" class="btn btn-sm btn-error">
                                                                    "Hard Delete"
                                                                </button>
                                                            </ActionForm>
                                                        </div>
                                                    </li>
                                                }
                                            }
                                        />
                                    }.into_any(),
                                    Err(e) => view! { <li class="text-error p-4">{format!("Error loading deleted users: {}", e)}</li> }.into_any()
                                })
                            }}
                        </Suspense>
                    </ul>
                </div>
            </div>
        </div>
        }.into_any()
                    }
                    Some(Ok(Some(_))) => {
                        view! {
                            <div class="alert alert-error max-w-lg mx-auto my-8">
                                <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                                <span>"Unauthorized: You must be an administrator to access User Management."</span>
                            </div>
                        }.into_any()
                    }
                    _ => view! {
                        <div class="flex justify-center p-8">
                            <span class="loading loading-spinner loading-md"></span>
                        </div>
                    }.into_any()
                }
            }}
        </RequireAuth>
    }
}
