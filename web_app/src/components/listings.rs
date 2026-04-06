use leptos::prelude::*;
use num_format::{Locale, ToFormattedString};
use rust_decimal::Decimal;
use web_app_common::components::villa_card::VillaCard;
use web_app_common::listings::listing_search_server;

fn format_decimal(value: Decimal) -> String {
    // Split into integer and fractional parts
    let int_part = value.trunc(); // e.g. 1234567
    let frac_part = value.fract(); // e.g. 0.89

    // Format the integer part with thousands separators
    let int_i64 = int_part.mantissa() / 10_i128.pow(int_part.scale());
    let formatted_int = (int_i64 as i64).to_formatted_string(&Locale::en);

    // Format the fractional part (strip leading "0.")
    if frac_part.is_zero() {
        formatted_int
    } else {
        let frac_str = format!("{:.2}", frac_part); // "0.89"
        let frac_digits = &frac_str[1..]; // ".89"
        format!("{}{}", formatted_int, frac_digits) // "1,234,567.89"
    }
}

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
                                view! { <div class="col-span-full text-center opacity-50 text-xl py-10">"No listings found in the database."</div> }.into_any()
                            } else {
                                view! {
                                    <For
                                        each=move || data.clone()
                                        key=|listing| listing.id
                                        children=move |listing| {
                                            view! {
                                                <VillaCard
                                                    title=listing.name.clone()
                                                    description=listing.description.clone().unwrap_or_else(|| "(No description)".to_string())
                                                    image_url=listing.primary_image_url.clone().unwrap_or_else(|| "https://images.unsplash.com/photo-1499793983690-e29da59ef1c2?ixlib=rb-4.0.3&auto=format&fit=crop&w=800&q=80".to_string())
                                                    price=listing.price_per_night
                                                        .map(|p| format_decimal(p))
                                                        .unwrap_or_else(|| "0.00".to_string())
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
