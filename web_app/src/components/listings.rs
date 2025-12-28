use crate::models::ListingResponse;
use leptos::prelude::*;

#[component]
pub fn Listings(listings: LocalResource<Result<Vec<ListingResponse>, String>>) -> impl IntoView {
    view! {
        <Suspense fallback=move || view! { <p>"Loading listings..."</p> }>
            {move || {
                listings.get().map(|result| match result {
                    Ok(items) => view! {
                        <ul>
                            <For
                                each=move || items.clone()
                                key=|listing| listing.id.clone()
                                children=move |listing| view! {
                                    <li style="margin-bottom: 10px; border: 1px solid #ccc; padding: 10px; border-radius: 5px;">
                                        <strong>{listing.name}</strong>
                                        <p>{listing.description.unwrap_or_default()}</p>
                                        <p>"Price: $" {format!("{:.2}", listing.price_per_night.unwrap_or(0.0))}</p>
                                    </li>
                                }
                            />
                        </ul>
                    }.into_any(),
                    Err(e) => view! { <p style="color: red;">"Error loading listings: " {e.to_string()}</p> }.into_any(),
                })
            }}
        </Suspense>
    }
}
