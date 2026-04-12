#[cfg(feature = "ssr")]
use actix_session::Session;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[server]
pub async fn login_traditional(email: String, password: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use web_app_common::api_client::{get_client, user_api_url};

        let url = format!("{}/api/v1/users/login", user_api_url());
        let audience = user_api_url();

        let payload = serde_json::json!({
            "email": email,
            "password": password
        });

        let response = get_client()
            .post(&url, &audience, &payload)
            .await
            .map_err(|e| ServerFnError::new(format!("API Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(ServerFnError::new(err_text));
            }
            return Err(ServerFnError::new(format!("Login failed: {}", err_text)));
        }

        let user_resp: common::models::UserResponse = response
            .json()
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to parse response: {}", e)))?;

        let session = leptos_actix::extract::<Session>()
            .await
            .map_err(|_| ServerFnError::new("Session not found"))?;

        session
            .insert("user_id", user_resp.id.to_string())
            .map_err(|_| ServerFnError::new("Failed to set session"))?;
        session
            .insert(
                "user_name",
                format!("{} {}", user_resp.first_name, user_resp.last_name),
            )
            .map_err(|_| ServerFnError::new("Failed to set session"))?;
        session
            .insert("user_email", user_resp.email.to_string())
            .map_err(|_| ServerFnError::new("Failed to set session"))?;

        leptos_actix::redirect("/");
    }

    Ok(())
}

#[server]
pub async fn login_passwordless(email: String, _code: String) -> Result<(), ServerFnError> {
    // Placeholder passwordless login logic
    #[cfg(feature = "ssr")]
    {
        let session = leptos_actix::extract::<Session>()
            .await
            .map_err(|_| ServerFnError::new("Session not found"))?;

        session
            .insert("user_id", "mock-uuid-p".to_string())
            .map_err(|_| ServerFnError::new("Failed to set session"))?;
        session
            .insert(
                "user_name",
                email.split('@').next().unwrap_or("User").to_string(),
            )
            .map_err(|_| ServerFnError::new("Failed to set session"))?;

        leptos_actix::redirect("/");
    }

    Ok(())
}

#[server]
pub async fn register(
    first_name: String,
    last_name: String,
    email: String,
    phone_number: Option<String>,
    password: Option<String>,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use common::models::{NewBookerProfile, NewUserRequest};
        use web_app_common::api_client::{get_client, user_api_url};

        let url = format!("{}/api/v1/users/", user_api_url());
        let audience = user_api_url();

        let req_data = NewUserRequest {
            email: email.clone(),
            password: password.unwrap_or_default(),
            first_name: first_name.clone(),
            last_name: last_name.clone(),
            phone_number: phone_number.clone(),
            is_active: true,
            is_verified: false,
            attributes: Some(serde_json::json!({
                "is_admin": false,
                "can_manage_listings": false,
                "can_manage_bookings": false
            })),
            roles: Some(vec!["Booker".to_string()]),
            booker_profile: Some(NewBookerProfile {
                emergency_contacts: None,
                booking_preferences: None,
                loyalty: None,
            }),
            host_profile: None,
        };

        let response = get_client()
            .post(&url, &audience, &req_data)
            .await
            .map_err(|e| ServerFnError::new(format!("API Request failed: {}", e)))?;

        if !response.status().is_success() {
            let err_text = response.text().await.unwrap_or_default();
            return Err(ServerFnError::new(format!(
                "Registration failed: {}",
                err_text
            )));
        }

        let user_resp: common::models::UserResponse = response
            .json()
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to parse response: {}", e)))?;

        #[cfg(feature = "ssr")]
        if let Some(code) = user_resp.verification_code {
            web_app_common::email::send_verification_email(&email, &first_name, &code)
                .await
                .map_err(|e| {
                    ServerFnError::new(format!("Failed to send verification email: {}", e))
                })?;
        }

        // Redirect to verification page
        leptos_actix::redirect(format!("/verify?email={}", email).as_str());
    }

    Ok(())
}

#[server]
pub async fn google_login() -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        // In a real app, this would redirect to Google's OAuth consent page
        leptos_actix::redirect("https://accounts.google.com/o/oauth2/v2/auth");
    }
    Ok(())
}

#[server]
pub async fn facebook_login() -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        // In a real app, this would redirect to Facebook's OAuth page
        leptos_actix::redirect("https://www.facebook.com/v12.0/dialog/oauth");
    }
    Ok(())
}

#[server]
pub async fn logout() -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let session = leptos_actix::extract::<Session>()
            .await
            .map_err(|_| ServerFnError::new("Session not found"))?;

        session.purge();
        leptos_actix::redirect("/");
    }
    Ok(())
}

#[server]
pub async fn get_current_user() -> Result<Option<UserProfile>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let session = try_extract_session().await?;

        let user_id = session.get::<String>("user_id").ok().flatten();
        let name = session.get::<String>("user_name").ok().flatten();
        let email = session.get::<String>("user_email").ok().flatten();

        if let (Some(id), Some(name), Some(email)) = (user_id, name, email) {
            Ok(Some(UserProfile { id, name, email }))
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
pub async fn verify_email_code(email: String, code: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use web_app_common::api_client::{get_client, user_api_url};

        let url = format!("{}/api/v1/users/verify", user_api_url());
        let audience = user_api_url();

        let payload = serde_json::json!({
            "email": email,
            "code": code
        });

        let response = get_client()
            .post(&url, &audience, &payload)
            .await
            .map_err(|e| ServerFnError::new(format!("API Request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(ServerFnError::new("Invalid or expired verification code"));
        }

        let user_resp: common::models::UserResponse = response
            .json()
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to parse response: {}", e)))?;

        // Automatically log them in
        let session = leptos_actix::extract::<Session>()
            .await
            .map_err(|_| ServerFnError::new("Session not found"))?;

        session
            .insert("user_id", user_resp.id.to_string())
            .map_err(|_| ServerFnError::new("Failed to set session"))?;
        session
            .insert(
                "user_name",
                format!("{} {}", user_resp.first_name, user_resp.last_name),
            )
            .map_err(|_| ServerFnError::new("Failed to set session"))?;
        session
            .insert("user_email", user_resp.email.to_string())
            .map_err(|_| ServerFnError::new("Failed to set session"))?;

        leptos_actix::redirect("/");
    }

    Ok(())
}

#[cfg(feature = "ssr")]
async fn try_extract_session() -> Result<Session, ServerFnError> {
    leptos_actix::extract::<Session>()
        .await
        .map_err(|_| ServerFnError::new("Session not found"))
}
