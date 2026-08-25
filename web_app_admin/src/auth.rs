#[cfg(feature = "ssr")]
use crate::api_client;
#[cfg(feature = "ssr")]
use actix_session::Session;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminSessionUser {
    pub id: String,
    pub name: String,
    pub email: String,
    pub is_admin: bool,
}

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
        let is_admin = user.roles.iter().any(|r| r.to_lowercase() == "admin");
        let is_host = user.roles.iter().any(|r| r.to_lowercase() == "host");

        if !is_admin && !is_host {
            return Err(ServerFnError::new(
                "Unauthorized: Admin or Host access required",
            ));
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
        session
            .insert("user_email", user.email.clone())
            .map_err(|_| ServerFnError::new("Failed to set session"))?;
        session
            .insert("is_admin", is_admin)
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
pub async fn get_current_session_user() -> Result<Option<AdminSessionUser>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let session = leptos_actix::extract::<Session>()
            .await
            .map_err(|_| ServerFnError::new("Session not found"))?;

        let user_id = session.get::<String>("user_id").unwrap_or(None);
        let user_name = session.get::<String>("user_name").unwrap_or(None);
        let user_email = session.get::<String>("user_email").unwrap_or(None);
        let is_admin = session
            .get::<bool>("is_admin")
            .unwrap_or(None)
            .unwrap_or(false);

        if let (Some(id), Some(name), Some(email)) = (user_id, user_name, user_email) {
            Ok(Some(AdminSessionUser {
                id,
                name,
                email,
                is_admin,
            }))
        } else if let Some(name) = session.get::<String>("user_name").unwrap_or(None) {
            let id = session
                .get::<String>("user_id")
                .unwrap_or_default()
                .unwrap_or_default();
            Ok(Some(AdminSessionUser {
                id,
                name,
                email: String::new(),
                is_admin,
            }))
        } else {
            Ok(None)
        }
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
