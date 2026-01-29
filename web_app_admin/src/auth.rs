#[cfg(feature = "ssr")]
use actix_session::Session;
#[cfg(feature = "ssr")]
use db_core::user::get_user_by_email;
#[cfg(feature = "ssr")]
use db_core::PgPool;
use leptos::prelude::*;

#[server]
pub async fn login(email: String, password: String) -> Result<(), ServerFnError> {
    // Get request parts
    let pool =
        use_context::<PgPool>().ok_or_else(|| ServerFnError::new("Database pool not found"))?;

    // Attempt to find user
    let user = get_user_by_email(&pool, &email)
        .await
        .map_err(|_| ServerFnError::new("Invalid credentials"))?;

    // Verify password
    let valid = bcrypt::verify(&password, &user.password_hash)
        .map_err(|_| ServerFnError::new("Password verification failed"))?;

    if !valid {
        return Err(ServerFnError::new("Invalid credentials"));
    }

    // Set session
    let session =
        use_context::<Session>().ok_or_else(|| ServerFnError::new("Session not found"))?;
    session
        .insert("user_id", user.id.to_string())
        .map_err(|_| ServerFnError::new("Failed to set session"))?;

    // Redirect to home
    leptos_actix::redirect("/home");

    Ok(())
}
