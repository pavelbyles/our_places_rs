#![recursion_limit = "256"]
#[cfg(feature = "ssr")]
pub mod api_client;
pub mod app;
pub mod auth;
pub mod components;
#[cfg(feature = "ssr")]
pub mod session_store;

#[cfg(feature = "hydrate")]
use crate::app::App;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}

#[cfg(test)]
mod tests {
    use crate::auth::AdminSessionUser;
    use crate::components::user::{CreateUserParams, UpdateUserParams};

    #[test]
    fn test_admin_session_user_serialization() {
        let user = AdminSessionUser {
            id: "user-123".to_string(),
            name: "Host User".to_string(),
            email: "host@example.com".to_string(),
            is_admin: false,
        };

        let json = serde_json::to_string(&user).unwrap();
        let deserialized: AdminSessionUser = serde_json::from_str(&json).unwrap();
        assert_eq!(user, deserialized);
        assert!(!deserialized.is_admin);
    }

    #[test]
    fn test_create_user_params_serialization() {
        let params = CreateUserParams {
            email: "newhost@example.com".to_string(),
            password: "password123".to_string(),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            phone_number: Some("+18765551234".to_string()),
            is_active: true,
            can_manage_bookings: true,
            can_manage_listings: true,
            is_admin: false,
            is_booker: false,
            is_host: true,
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: CreateUserParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, deserialized);
        assert!(deserialized.is_host);
        assert!(!deserialized.is_admin);
    }

    #[test]
    fn test_update_user_params_serialization() {
        let params = UpdateUserParams {
            id: "user-456".to_string(),
            email: "updated@example.com".to_string(),
            password: "".to_string(),
            first_name: "Jane".to_string(),
            last_name: "Smith".to_string(),
            phone_number: None,
            is_active: true,
            can_manage_bookings: false,
            can_manage_listings: true,
            is_admin: false,
            is_booker: true,
            is_host: true,
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: UpdateUserParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, deserialized);
        assert!(deserialized.is_booker);
        assert!(deserialized.is_host);
    }
}
