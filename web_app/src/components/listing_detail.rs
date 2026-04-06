use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

#[component]
#[allow(non_snake_case)]
pub fn ListingDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id").unwrap_or_default());

    match Uuid::parse_str(&id()) {
        Ok(uuid) => {
            // Valid UUID
        }
        Err(_) => {
            // query by slug
        }
    }

    view! {
        <div>
            <h1>"Listing Detail"</h1>
            <p>"Listing ID: " {id}</p>
        </div>
    }
}
