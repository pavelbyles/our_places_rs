use leptos::prelude::*;
use num_format::{Locale, ToFormattedString};
use rust_decimal::prelude::ToPrimitive;
use web_app_common::components::villa_card::VillaCard;
use web_app_common::listings::listing_search_server;

#[component]
#[allow(non_snake_case)]
pub fn ListingsPage() -> impl IntoView {
    let (search_trigger, set_search_trigger) = signal(0);

    let listings = Resource::new(
        move || search_trigger.get(),
        |_| async move { listing_search_server(None, None, None, None).await },
    );

    view! {
        <div class="flex flex-col items-center w-full mt-10 px-4 gap-12">
            <div class="join shadow-md">
                <input
                    class="input input-bordered join-item w-full max-w-xs"
                    placeholder="Search"
                />

                <select class="select select-bordered join-item">
                    <option disabled selected>"Filter"</option>
                    <option>"Apartment"</option>
                    <option>"Villa"</option>
                    <option>"House"</option>
                </select>

                <button
                    class="btn btn-primary join-item"
                    on:click=move |_| set_search_trigger.update(|n| *n += 1)
                >
                    "Search"
                </button>
            </div>

            <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 w-full max-w-5xl pb-10">
                <Suspense fallback=move || view! { <span class="loading loading-spinner loading-lg col-span-full mx-auto mt-10"></span> }>
                    {move || listings.get().map(|res| match res {
                        Ok(data) => {
                            if data.is_empty() {
                                view! { <div class="col-span-full text-center opacity-50 text-xl py-10">"No listings found"</div> }.into_any()
                            } else {
                                view! {
                                    <For
                                        each=move || data.clone()
                                        key=|listing| listing.id
                                        children=move |listing| {
                                            view! {
                                                <VillaCard
                                                    title=listing.name.clone()
                                                    image_url=listing.primary_image_url.clone().unwrap_or_else(|| "https://images.unsplash.com/photo-1499793983690-e29da59ef1c2?ixlib=rb-4.0.3&auto=format&fit=crop&w=800&q=80".to_string())
                                                    price=listing.price_per_night
                                                        .map(|p| p.to_i64().unwrap().to_formatted_string(&Locale::en))
                                                        .unwrap_or_else(|| "0.00".to_string())
                                                    max_guests=listing.max_guests
                                                    bedrooms=listing.bedrooms
                                                    full_bathrooms=listing.full_bathrooms
                                                    country=listing.country.clone()
                                                    city=listing.city.clone()
                                                />
                                            }
                                        }
                                    />
                                }.into_any()
                            }
                        },
                        Err(e) => view! {
                            <div class="alert alert-error col-span-full shadow-md">
                                <span>"Error loading listings: " {e.to_string()}</span>
                            </div>
                        }.into_any(),
                    })}
                </Suspense>
            </div>
        </div>
    }
}
