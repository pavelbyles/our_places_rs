#[cfg(feature = "ssr")]
use crate::api_client;
#[cfg(feature = "ssr")]
use actix_session::Session;
use leptos::prelude::*;

#[cfg(feature = "ssr")]
use common::auth::UserAttribute;

#[server]
pub async fn login(email: String, password: String) -> Result<(), ServerFnError> {
    // Attempt to login via API
    let user = api_client::login(&email, &password)
        .await
        .map_err(|e| ServerFnError::new(format!("Login failed: {}", e)))?;

    // Check for admin attribute
    let is_admin = user
        .attributes
        .get(UserAttribute::IsAdmin.as_str())
        .and_then(|v: &serde_json::Value| v.as_bool())
        .unwrap_or(false);

    if !is_admin {
        return Err(ServerFnError::new("Unauthorized: Admin access required"));
    }

    // Set session
    let session = leptos_actix::extract::<Session>()
        .await
        .map_err(|_| ServerFnError::new("Session not found"))?;

    session
        .insert("user_id", user.id.to_string())
        .map_err(|_| ServerFnError::new("Failed to set session"))?;
    session
        .insert("user_name", user.first_name.clone())
        .map_err(|_| ServerFnError::new("Failed to set session"))?;

    // Redirect to admin
    leptos_actix::redirect("/admin");

    Ok(())
}

#[server]
pub async fn get_current_user() -> Result<Option<String>, ServerFnError> {
    let session = leptos_actix::extract::<Session>()
        .await
        .map_err(|_| ServerFnError::new("Session not found"))?;

    Ok(session.get::<String>("user_name").unwrap_or(None))
}

#[server]
pub async fn logout() -> Result<(), ServerFnError> {
    let session = leptos_actix::extract::<Session>()
        .await
        .map_err(|_| ServerFnError::new("Session not found"))?;

    session.purge();
    leptos_actix::redirect("/login");
    Ok(())
}
