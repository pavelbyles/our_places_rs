#[cfg(feature = "ssr")]
use crate::api_client;
#[cfg(feature = "ssr")]
use actix_session::Session;
use leptos::prelude::*;

#[server]
pub async fn login(email: String, password: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        // Attempt to login via API
        let user = api_client::login(&email, &password)
            .await
            .map_err(|e| match e {
                api_client::ClientError::RequestFailed(status)
                    if status == reqwest::StatusCode::UNAUTHORIZED =>
                {
                    ServerFnError::new("Invalid credentials")
                }
                _ => ServerFnError::new(format!("Login failed: {}", e)),
            })?;

        // Check for admin or host role
        let is_authorized = user.roles.iter().any(|r| {
            let role = r.to_lowercase();
            role == "admin" || role == "host"
        });

        if !is_authorized {
            return Err(ServerFnError::new("Unauthorized: Admin or Host access required"));
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
    }

    Ok(())
}

#[server]
pub async fn get_current_user() -> Result<Option<String>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let session = leptos_actix::extract::<Session>()
            .await
            .map_err(|_| ServerFnError::new("Session not found"))?;

        return Ok(session.get::<String>("user_name").unwrap_or(None));
    }
    #[cfg(not(feature = "ssr"))]
    {
        Ok(None)
    }
}

#[server]
pub async fn logout() -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let session = leptos_actix::extract::<Session>()
            .await
            .map_err(|_| ServerFnError::new("Session not found"))?;

        session.purge();
        leptos_actix::redirect("/login");
    }
    Ok(())
}
