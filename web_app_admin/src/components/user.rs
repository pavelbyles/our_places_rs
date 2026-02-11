use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewUserData {
    pub email: String,
    pub password: String,
    pub first_name: String,
    pub last_name: String,
    pub phone_number: Option<String>,
    pub is_active: bool,
    pub is_admin: bool,
    pub can_manage_users: bool,
    pub can_manage_listings: bool,
    pub can_manage_bookings: bool,
}

#[component]
pub fn UsersPage() -> impl IntoView {
    let add_user_action = ServerAction::<AddUser>::new();

    view! {
        <h1>"Users Page"</h1>
        <ActionForm action=add_user_action>
            <button type="submit">"Add User"</button>
        </ActionForm>
    }
}

#[server]
pub async fn add_user(_data: NewUserData) -> Result<(), ServerFnError> {
    Ok(())
}
