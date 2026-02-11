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
