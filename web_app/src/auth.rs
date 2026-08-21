#[cfg(feature = "ssr")]
use actix_session::Session;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UserProfile {
    pub id: String,
    pub name: String,
    pub email: String,
    pub phone_number: Option<String>,
    pub default_currency: String,
}

#[cfg(feature = "ssr")]
pub struct Unvalidated;
#[cfg(feature = "ssr")]
pub struct Validated {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub phone_number: Option<String>,
    pub default_currency: String,
}

#[cfg(feature = "ssr")]
pub struct AppSession<State> {
    pub session: Session,
    pub state: State,
}

#[cfg(feature = "ssr")]
impl AppSession<Unvalidated> {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            state: Unvalidated,
        }
    }

    pub fn verify(self) -> Result<AppSession<Validated>, ServerFnError> {
        let user_id = self
            .session
            .get::<String>("user_id")
            .ok()
            .flatten()
            .ok_or_else(|| ServerFnError::new("Unauthorized"))?;
        let name = self
            .session
            .get::<String>("user_name")
            .ok()
            .flatten()
            .ok_or_else(|| ServerFnError::new("Unauthorized"))?;
        let email = self
            .session
            .get::<String>("user_email")
            .ok()
            .flatten()
            .ok_or_else(|| ServerFnError::new("Unauthorized"))?;
        let phone_number = self.session.get::<String>("user_phone").ok().flatten();
        let default_currency = self
            .session
            .get::<String>("user_default_currency")
            .ok()
            .flatten()
            .unwrap_or_else(|| "USD".to_string());

        Ok(AppSession {
            session: self.session,
            state: Validated {
                user_id,
                name,
                email,
                phone_number,
                default_currency,
            },
        })
    }
}

#[cfg(feature = "ssr")]
impl AppSession<Validated> {
    pub fn extract_user(&self) -> UserProfile {
        UserProfile {
            id: self.state.user_id.clone(),
            name: self.state.name.clone(),
            email: self.state.email.clone(),
            phone_number: self.state.phone_number.clone(),
            default_currency: self.state.default_currency.clone(),
        }
    }
}

#[server]
pub async fn login_traditional(email: String, password: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use web_app_common::api_client::{get_client, user_api_audience, user_api_url};

        let url = format!("{}/api/v1/users/login", user_api_url());
        let audience = user_api_audience();

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
            let err_body: serde_json::Value = response
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({ "error": "Unknown login error" }));
            let err_msg = err_body["error"].as_str().unwrap_or("Login failed");

            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(ServerFnError::new(err_msg));
            }
            return Err(ServerFnError::new(err_msg));
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
        if let Some(phone) = user_resp.phone_number {
            session
                .insert("user_phone", phone)
                .map_err(|_| ServerFnError::new("Failed to set session"))?;
        }
        session
            .insert("user_default_currency", user_resp.default_currency)
            .map_err(|_| ServerFnError::new("Failed to set session"))?;

        let transferred_booking = try_transfer_pending_booking(&session, user_resp.id).await;

        if let Some(booking_id) = transferred_booking {
            leptos_actix::redirect(&format!("/checkout/{}", booking_id));
        } else {
            leptos_actix::redirect("/");
        }
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
        session
            .insert("user_default_currency", "USD".to_string())
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
        use web_app_common::api_client::{get_client, user_api_audience, user_api_url};

        let url = format!("{}/api/v1/users/", user_api_url());
        let audience = user_api_audience();

        let req_data = NewUserRequest {
            email: email.clone(),
            password: password.unwrap_or_default(),
            first_name: first_name.clone(),
            last_name: last_name.clone(),
            phone_number: phone_number.clone(),
            is_active: true,
            is_verified: false,
            attributes: Some(serde_json::json!({
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
            default_currency: Some("USD".to_string()),
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
        leptos_actix::redirect(format!("/verify?email={}", urlencoding::encode(&email)).as_str());
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
        let app_session = AppSession::new(session);

        match app_session.verify() {
            Ok(validated) => Ok(Some(validated.extract_user())),
            Err(_) => Ok(None),
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
        use web_app_common::api_client::{get_client, user_api_audience, user_api_url};

        let url = format!("{}/api/v1/users/verify", user_api_url());
        let audience = user_api_audience();

        let payload = serde_json::json!({
            "email": email,
            "code": code
        });

        let response = get_client()
            .post(&url, &audience, &payload)
            .await
            .map_err(|e| ServerFnError::new(format!("API Request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&error_text) {
                if let Some(msg) = json_val.get("message").and_then(|m| m.as_str()) {
                    return Err(ServerFnError::new(msg.to_string()));
                }
            }
            return Err(ServerFnError::new(if error_text.is_empty() {
                "Invalid or expired verification code".to_string()
            } else {
                error_text
            }));
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
        session
            .insert("user_default_currency", user_resp.default_currency)
            .map_err(|_| ServerFnError::new("Failed to set session"))?;

        let transferred_booking = try_transfer_pending_booking(&session, user_resp.id).await;

        if let Some(booking_id) = transferred_booking {
            leptos_actix::redirect(&format!("/checkout/{}", booking_id));
        } else {
            leptos_actix::redirect("/");
        }
    }

    Ok(())
}

#[server]
pub async fn resend_verification_code(email: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use web_app_common::api_client::{get_client, user_api_audience, user_api_url};

        let url = format!("{}/api/v1/users/resend-verification", user_api_url());
        let audience = user_api_audience();

        let payload = serde_json::json!({
            "email": email
        });

        let response = get_client()
            .post(&url, &audience, &payload)
            .await
            .map_err(|e| ServerFnError::new(format!("API Request failed: {}", e)))?;

        if !response.status().is_success() {
            let err_text = response.text().await.unwrap_or_default();
            return Err(ServerFnError::new(format!(
                "Failed to resend code: {}",
                err_text
            )));
        }

        let user_resp: common::models::UserResponse = response
            .json()
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to parse response: {}", e)))?;

        if let Some(code) = user_resp.verification_code {
            web_app_common::email::send_verification_email(&email, &user_resp.first_name, &code)
                .await
                .map_err(|e| {
                    ServerFnError::new(format!("Failed to send verification email: {}", e))
                })?;
        }
    }

    Ok(())
}

#[server]
pub async fn request_password_change(
    email: String,
    current_password: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use web_app_common::api_client::{get_client, user_api_audience, user_api_url};

        let url = format!("{}/api/v1/users/profile/password/request", user_api_url());
        let audience = user_api_audience();

        let payload = serde_json::json!({
            "email": email,
            "current_password": current_password
        });

        let response = get_client()
            .post(&url, &audience, &payload)
            .await
            .map_err(|e| ServerFnError::new(format!("API Request failed: {}", e)))?;

        if !response.status().is_success() {
            let err_text = response.text().await.unwrap_or_default();
            return Err(ServerFnError::new(format!(
                "Failed to request password change: {}",
                err_text
            )));
        }

        let user_resp: common::models::UserResponse = response
            .json()
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to parse response: {}", e)))?;

        if let Some(code) = user_resp.verification_code {
            web_app_common::email::send_password_change_email(&email, &user_resp.first_name, &code)
                .await
                .map_err(|e| {
                    ServerFnError::new(format!("Failed to send password change email: {}", e))
                })?;
        }
    }
    Ok(())
}

#[server]
pub async fn confirm_password_change(
    email: String,
    code: String,
    new_password: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use web_app_common::api_client::{get_client, user_api_audience, user_api_url};

        let url = format!("{}/api/v1/users/profile/password/confirm", user_api_url());
        let audience = user_api_audience();

        let payload = serde_json::json!({
            "email": email,
            "code": code,
            "new_password": new_password
        });

        let response = get_client()
            .post(&url, &audience, &payload)
            .await
            .map_err(|e| ServerFnError::new(format!("API Request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(ServerFnError::new(
                "Failed to confirm password change. Check code.",
            ));
        }

        let session = try_extract_session().await?;
        session.purge();
        leptos_actix::redirect("/");
    }
    Ok(())
}

#[server]
pub async fn change_email(
    email: String,
    current_password: String,
    new_email: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use web_app_common::api_client::{get_client, user_api_audience, user_api_url};

        let url = format!("{}/api/v1/users/profile/email", user_api_url());
        let audience = user_api_audience();

        let payload = serde_json::json!({
            "email": email,
            "current_password": current_password,
            "new_email": new_email
        });

        let response = get_client()
            .post(&url, &audience, &payload)
            .await
            .map_err(|e| ServerFnError::new(format!("API Request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(ServerFnError::new("Failed to change email."));
        }

        let user_resp: common::models::UserResponse = response
            .json()
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to parse response: {}", e)))?;

        if let Some(code) = user_resp.verification_code {
            web_app_common::email::send_verification_email(
                &new_email,
                &user_resp.first_name,
                &code,
            )
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to send verification email: {}", e)))?;
        }

        let session = try_extract_session().await?;
        session.purge();
        leptos_actix::redirect(&format!("/verify?email={}", new_email));
    }
    Ok(())
}

#[server]
pub async fn deactivate_account(
    email: String,
    current_password: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use web_app_common::api_client::{get_client, user_api_audience, user_api_url};

        let url = format!("{}/api/v1/users/profile/deactivate", user_api_url());
        let audience = user_api_audience();

        let payload = serde_json::json!({
            "email": email,
            "current_password": current_password
        });

        let response = get_client()
            .post(&url, &audience, &payload)
            .await
            .map_err(|e| ServerFnError::new(format!("API Request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(ServerFnError::new("Failed to deactivate account."));
        }

        let session = try_extract_session().await?;
        session.purge();
        leptos_actix::redirect("/");
    }
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn try_extract_session() -> Result<Session, ServerFnError> {
    leptos_actix::extract::<Session>()
        .await
        .map_err(|_| ServerFnError::new("Session not found"))
}

#[cfg(feature = "ssr")]
pub async fn try_transfer_pending_booking(
    session: &Session,
    new_user_id: uuid::Uuid,
) -> Option<uuid::Uuid> {
    let booking_id = if let Ok(Some(booking_id_str)) = session.get::<String>("pending_booking_id") {
        uuid::Uuid::parse_str(&booking_id_str).ok()
    } else {
        None
    };

    if let Some(bid) = booking_id {
        if web_app_common::bookings::transfer_booking_api(bid, new_user_id)
            .await
            .is_ok()
        {
            tracing::info!(
                "Transferred pending booking {} to user {}",
                bid,
                new_user_id
            );
        }
        session.remove("pending_booking_id");
        return Some(bid);
    }
    None
}
