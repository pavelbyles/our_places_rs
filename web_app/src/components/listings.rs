use crate::components::card::VillaCard;
use crate::models::ListingResponse;
use leptos::prelude::*;

#[component]
pub fn Listings(listings: LocalResource<Result<Vec<ListingResponse>, String>>) -> impl IntoView {
    view! {
        <Suspense fallback=move || view! { <p>"Loading listings..."</p> }>
            {move || {
                listings.get().map(|result| match result {
                    Ok(items) => view! {
                        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 p-4">
                            <For
                                each=move || items.clone()
                                key=|listing| listing.id.clone()
                                children=move |listing| view! {
                                    <VillaCard
                                        name=listing.name
                                        location=listing.country
                                        beds=2
                                        min_stay=2
                                        image_url="https://images.unsplash.com/photo-1520250497591-112f2f40a3f4?ixlib=rb-4.0.3&ixid=MnwxMjA3fDB8MHxwaG90by1wYWdlfHx8fGVufDB8fHx8&auto=format&fit=crop&w=2340&q=80"
                                    />
                                }
                            />
                        </div>
                    }.into_any(),
                    Err(e) => view! { <p style="color: red;">"Error loading listings: " {e.to_string()}</p> }.into_any(),
                })
            }}
        </Suspense>
    }
}
