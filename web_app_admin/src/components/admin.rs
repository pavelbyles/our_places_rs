#[cfg(feature = "ssr")]
use common::models::{NewUserRequest, UpdateUserRequest};
use leptos::form::ActionForm;
use leptos::html::Input;
use leptos::prelude::*;

#[server]
pub async fn create_user_server(
    email: String,
    password: String,
    first_name: String,
    last_name: String,
    phone_number: Option<String>,
    #[server(default)] is_active: bool,
    #[server(default)] can_manage_bookings: bool,
    #[server(default)] can_manage_listings: bool,
    #[server(default)] is_admin: bool,
    #[server(default)] is_booker: bool,
    #[server(default)] is_host: bool,
) -> Result<(), ServerFnError> {
    let mut roles = Vec::new();
    if is_booker {
        roles.push("booker".to_string());
    }
    if is_host {
        roles.push("host".to_string());
    }

    let attributes = serde_json::json!({
        "can_manage_bookings": can_manage_bookings,
        "can_manage_listings": can_manage_listings,
        "is_admin": is_admin
    });

    let request = NewUserRequest {
        email,
        password,
        first_name,
        last_name,
        phone_number,
        is_active,
        attributes: Some(attributes),
        roles: Some(roles),
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

#[server]
pub async fn update_user_server(
    id: String,
    email: String,
    password: String,
    first_name: String,
    last_name: String,
    phone_number: Option<String>,
    #[server(default)] is_active: bool,
    #[server(default)] can_manage_bookings: bool,
    #[server(default)] can_manage_listings: bool,
    #[server(default)] is_admin: bool,
    #[server(default)] is_booker: bool,
    #[server(default)] is_host: bool,
) -> Result<(), ServerFnError> {
    let mut roles = Vec::new();
    if is_booker {
        roles.push("booker".to_string());
    }
    if is_host {
        roles.push("host".to_string());
    }

    let attributes = serde_json::json!({
        "can_manage_bookings": can_manage_bookings,
        "can_manage_listings": can_manage_listings,
        "is_admin": is_admin
    });

    let request = UpdateUserRequest {
        email: Some(email),
        password: Some(password),
        first_name: Some(first_name),
        last_name: Some(last_name),
        phone_number: phone_number,
        is_active: Some(is_active),
        attributes: Some(attributes),
        roles: Some(roles),
        booker_profile: None,
        host_profile: None,
    };

    let client = reqwest::Client::new();
    let res = client
        .patch(format!("http://localhost:8083/api/v1/users/user/{}", id))
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
            "Failed to update user: {}",
            res.status()
        )))
    }
}

#[server]
pub async fn get_users_server(
    search: Option<String>,
) -> Result<Vec<common::models::UserResponse>, ServerFnError> {
    let client = reqwest::Client::new();
    let api_url =
        std::env::var("USER_API_URL").unwrap_or_else(|_| "http://localhost:8083".to_string());
    let mut url = format!("{}/api/v1/users/?page=1&per_page=20", api_url);

    if let Some(s) = search {
        if !s.is_empty() {
            url.push_str(&format!("&search={}", s));
        }
    }

    let res = client
        .get(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send()
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

#[component]
pub fn AdminPage() -> impl IntoView {
    let create_user = ServerAction::<CreateUserServer>::new();
    let update_user = ServerAction::<UpdateUserServer>::new();
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

    // User List State
    let (search, set_search) = signal(String::new());
    let users_resource = Resource::new(
        move || search.get(),
        |s| async move { get_users_server(Some(s)).await },
    );

    let is_form_valid = move || {
        let e = email.get();
        // Basic check: must contain '@' and domain must contain '.'
        let email_format_valid = e.contains('@')
            && e.split('@')
                .nth(1)
                .map_or(false, |domain| domain.contains('.'));
        !e.is_empty()
            && email_format_valid
            && !first_name.get().is_empty()
            && !last_name.get().is_empty()
    };

    view! {
        <h1>"Admin Page"</h1>
        <div class="tabs tabs-lift">
            <input type="radio" name="my_tabs_3" class="tab" aria-label="Add User" checked="checked" />
            <div class="tab-content bg-base-100 border-base-300 p-6">
                <ActionForm action=create_user attr:class="form-control w-full max-w-xs space-y-4">
                    <hidden-input name="id" value="" />
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
                                    <td><div class="flex justify-end w-full"><input type="checkbox" name="can_manage_bookings" class="checkbox checkbox-sm" value="true" /></div></td>
                                </tr>
                                <tr>
                                    <td class="text-left font-normal">"Can Manage Listings"</td>
                                    <td><div class="flex justify-end w-full"><input type="checkbox" name="can_manage_listings" class="checkbox checkbox-sm" value="true" /></div></td>
                                </tr>
                                <tr>
                                    <td class="text-left font-normal">"Is Admin"</td>
                                    <td><div class="flex justify-end w-full"><input type="checkbox" name="is_admin" class="checkbox checkbox-sm" value="true" /></div></td>
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
                                    <td><div class="flex justify-end w-full"><input type="checkbox" name="is_booker" class="checkbox checkbox-sm" value="true" /></div></td>
                                </tr>
                                <tr>
                                    <td class="text-left font-normal">"Is Host"</td>
                                    <td><div class="flex justify-end w-full"><input type="checkbox" name="is_host" class="checkbox checkbox-sm" value="true" /></div></td>
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
                            <input type="hidden" name="id" node_ref=id_ref />
                            <div>
                                <label class="label">
                                    <span class="label-text">"Email"</span>
                                </label>
                                <input type="email" name="email" placeholder="Email" class="input input-bordered w-full max-w-xs" required
                                    node_ref=email_ref
                                    on:input=move |_| set_email.set(email_ref.get().expect("input").value())
                                />
                            </div>

                            <div>
                                <label class="input">
                                    <input type="password" name="password" placeholder="Password" class="input input-bordered w-full max-w-xs" />
                                    <span class="badge badge-neutral badge-xs">Optional</span>
                                </label>
                            </div>

                            <div>
                                <label class="label">
                                    <span class="label-text">"First Name"</span>
                                </label>
                                <input type="text" name="first_name" placeholder="First Name" class="input input-bordered w-full max-w-xs" required
                                    node_ref=first_name_ref
                                    on:input=move |_| set_first_name.set(first_name_ref.get().expect("input").value())
                                />
                            </div>

                            <div>
                                <label class="label">
                                    <span class="label-text">"Last Name"</span>
                                </label>
                                <input type="text" name="last_name" placeholder="Last Name" class="input input-bordered w-full max-w-xs" required
                                    node_ref=last_name_ref
                                    on:input=move |_| set_last_name.set(last_name_ref.get().expect("input").value())
                                />
                            </div>

                            <div>
                                <label class="label">
                                    <span class="label-text">"Phone Number"</span>
                                </label>
                                <input type="tel" name="phone_number" placeholder="Phone Number" class="input input-bordered w-full max-w-xs" node_ref=phone_number_ref />
                            </div>

                            <div class="form-control">
                                <label class="label cursor-pointer">
                                    <span class="label-text">"Is Active"</span>
                                    <input type="checkbox" name="is_active" class="checkbox" checked="checked" value="true" node_ref=is_active_ref />
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
                                            <td><div class="flex justify-end w-full"><input type="checkbox" name="can_manage_bookings" class="checkbox checkbox-sm" value="true" node_ref=can_manage_bookings_ref /></div></td>
                                        </tr>
                                        <tr>
                                            <td class="text-left font-normal">"Can Manage Listings"</td>
                                            <td><div class="flex justify-end w-full"><input type="checkbox" name="can_manage_listings" class="checkbox checkbox-sm" value="true" node_ref=can_manage_listings_ref /></div></td>
                                        </tr>
                                        <tr>
                                            <td class="text-left font-normal">"Is Admin"</td>
                                            <td><div class="flex justify-end w-full"><input type="checkbox" name="is_admin" class="checkbox checkbox-sm" value="true" node_ref=is_admin_ref /></div></td>
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
                                            <td><div class="flex justify-end w-full"><input type="checkbox" name="is_booker" class="checkbox checkbox-sm" value="true" node_ref=is_booker_ref /></div></td>
                                        </tr>
                                        <tr>
                                            <td class="text-left font-normal">"Is Host"</td>
                                            <td><div class="flex justify-end w-full"><input type="checkbox" name="is_host" class="checkbox checkbox-sm" value="true" node_ref=is_host_ref /></div></td>
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
                                                                    set_email.set(user_clone.email.clone());
                                                                    set_first_name.set(user_clone.first_name.clone());
                                                                    set_last_name.set(user_clone.last_name.clone());

                                                                    if let Some(ref input) = id_ref.get() {
                                                                        input.set_value(&user_clone.id.to_string());
                                                                    }
                                                                    if let Some(ref input) = email_ref.get() {
                                                                        input.set_value(&user_clone.email);
                                                                    }
                                                                    if let Some(ref input) = first_name_ref.get() {
                                                                        input.set_value(&user_clone.first_name);
                                                                    }
                                                                    if let Some(ref input) = last_name_ref.get() {
                                                                        input.set_value(&user_clone.last_name);
                                                                    }
                                                                    if let Some(ref input) = phone_number_ref.get() {
                                                                        input.set_value(&user_clone.phone_number.clone().unwrap_or_default());
                                                                    }
                                                                    if let Some(ref input) = is_active_ref.get() {
                                                                        input.set_checked(user_clone.is_active);
                                                                    }

                                                                    // Populate Attributes
                                                                    let attrs = &user_clone.attributes;
                                                                    if let Some(ref input) = can_manage_bookings_ref.get() {
                                                                        input.set_checked(attrs.get("can_manage_bookings").and_then(|v| v.as_bool()).unwrap_or(false));
                                                                    }
                                                                    if let Some(ref input) = can_manage_listings_ref.get() {
                                                                        input.set_checked(attrs.get("can_manage_listings").and_then(|v| v.as_bool()).unwrap_or(false));
                                                                    }
                                                                    if let Some(ref input) = is_admin_ref.get() {
                                                                        input.set_checked(attrs.get("is_admin").and_then(|v| v.as_bool()).unwrap_or(false));
                                                                    }

                                                                    // Populate Roles
                                                                    let roles = &user_clone.roles;
                                                                    if let Some(ref input) = is_booker_ref.get() {
                                                                        input.set_checked(roles.contains(&"booker".to_string()));
                                                                    }
                                                                    if let Some(ref input) = is_host_ref.get() {
                                                                        input.set_checked(roles.contains(&"host".to_string()));
                                                                    }
                                                                }
                                                            >
                                                                <div><img class="size-10 rounded-box" src="https://img.daisyui.com/images/profile/demo/1@94.webp"/></div>
                                                                <div>
                                                                    <div>{format!("{} {}", user_clone2.first_name, user_clone2.last_name)}</div>
                                                                    <div class="text-xs uppercase font-semibold opacity-60">{user_clone2.email}</div>
                                                                </div>
                                                                <button class="btn btn-square btn-ghost">
                                                                     <svg class="size-[1.2em]" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g stroke-linejoin="round" stroke-linecap="round" stroke-width="2" fill="none" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" d="m16.862 4.487 1.687-1.688a1.875 1.875 0 1 1 2.652 2.652L6.832 19.82a4.5 4.5 0 0 1-1.897 1.13l-2.685.8.8-2.685a4.5 4.5 0 0 1 1.13-1.897L16.863 4.487Zm0 0L19.5 7.125" /></g></svg>
                                                                </button>
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

            <input type="radio" name="my_tabs_3" class="tab" aria-label="Tab 3" />
            <div class="tab-content bg-base-100 border-base-300 p-6">Tab content 3</div>
        </div>
    }
}
