pub mod api_client;
pub mod auth;
pub mod client;
pub mod components;
pub mod layout;
pub mod theme;

pub use client::{get_api_client, TopcoatApiClient};

pub use auth::{AuthUser, auth_init_script, get_authenticated_admin, get_authenticated_guest};

pub use components::currency_selector::currency_selector;
pub use components::price_breakdown::price_breakdown;
pub use components::responsive_image::responsive_image;
pub use components::star_rating::star_rating;
pub use components::villa_card::villa_card;
pub use layout::guest_base_layout;
pub use theme::{theme_init_script, theme_toggle, theme_toggle_script};
