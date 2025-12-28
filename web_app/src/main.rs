use leptos::prelude::*;

mod api;
mod components;
mod models;

use api::fetch_listings;
use components::listings::Listings;

#[component]
pub fn App() -> impl IntoView {
    let listings = LocalResource::new(fetch_listings);

    view! {
        <div style="font-family: sans-serif; padding: 20px;">
            <h1>"Our Places - Listings"</h1>
            <Listings listings=listings />
        </div>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(|| view! { <App/> });
}
