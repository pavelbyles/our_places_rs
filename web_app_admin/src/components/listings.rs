use crate::components::protected::RequireAuth;
use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use std::collections::HashSet;

#[server]
pub async fn listing_search_server(
    name: Option<String>,
    owner_email: Option<String>,
    listing_structure: Option<Vec<String>>,
    max_price: Option<f64>,
) -> Result<Vec<common::models::ListingResponse>, ServerFnError> {
    let api_url = crate::api_client::listing_api_url();
    let mut url = format!("{}/api/v1/listings/?page=1&per_page=20", api_url);

    if let Some(s) = name {
        if !s.is_empty() {
            url.push_str(&format!("&name={}", s));
        }
    }

    if let Some(s) = owner_email {
        if !s.is_empty() {
            url.push_str(&format!("&owner={}", s));
        }
    }

    if let Some(structures) = listing_structure {
        if !structures.is_empty() {
            let joined = structures.join(",");
            url.push_str(&format!("&structure_type={}", joined));
        }
    }

    if let Some(s) = max_price {
        if s > 0.0 {
            url.push_str(&format!("&max_price={}", s));
        }
    }

    let res = crate::api_client::get_client()
        .get(&url, &api_url)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !res.status().is_success() {
        return Err(ServerFnError::new(format!(
            "Failed to fetch listings: {}",
            res.status()
        )));
    }

    let listings: Vec<common::models::ListingResponse> = res
        .json()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(listings)
}

#[component]
pub fn ListingsPage() -> impl IntoView {
    let listing_search = ServerAction::<ListingSearchServer>::new();
    let (name, set_name) = signal(None::<String>);
    let (owner_email, set_owner_email) = signal(None::<String>);
    let (max_price, set_max_price) = signal(Some(0.0));
    let (selected_structures, set_selected_structures) = signal(HashSet::<String>::new());

    let listings = Memo::new(move |_| {
        listing_search
            .value()
            .get()
            .unwrap_or_else(|| Ok(vec![]))
            .unwrap_or_default()
    });

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let structures = selected_structures.get();
        let structure_vec: Vec<String> = structures.into_iter().collect();

        let structure_arg = if structure_vec.is_empty() {
            None
        } else {
            Some(structure_vec)
        };

        listing_search.dispatch(ListingSearchServer {
            name: name.get(),
            owner_email: owner_email.get(),
            listing_structure: structure_arg,
            max_price: max_price.get(),
        });
    };

    let toggle_structure = move |structure: String| {
        set_selected_structures.update(|set| {
            if set.contains(&structure) {
                set.remove(&structure);
            } else {
                set.insert(structure);
            }
        });
    };

    view! {
        <RequireAuth>
             <h1>"Listings Page"</h1>
             <div class="flex w-full flex-col lg:flex-row">
            <div class="card bg-base-300 rounded-box grid grow p-4">
                <div class="flex flex-col mb-4">
                <div class="flex flex-col mb-4">
                    <form on:submit=on_submit class="form-control w-full space-y-4">
                        <div class="flex flex-wrap gap-4 items-end">
                            <div class="form-control w-full max-w-xs">
                                <label class="label">
                                    <span class="label-text">Owner Email</span>
                                </label>
                                <label class="input input-bordered flex items-center gap-2">
                                    <svg class="h-[1em] opacity-50" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                                        <g stroke-linejoin="round" stroke-linecap="round" stroke-width="2.5" fill="none" stroke="currentColor">
                                        <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"></path>
                                        <circle cx="12" cy="7" r="4"></circle>
                                        </g>
                                    </svg>
                                    <input
                                        type="email"
                                        class="grow"
                                        placeholder="username@domain.com"
                                        on:input=move |ev| set_owner_email.set(Some(event_target_value(&ev)))
                                        prop:value=move || owner_email.get().unwrap_or_default()
                                    />
                                </label>
                            </div>

                            <div class="form-control w-full max-w-xs">
                                <label class="label">
                                    <span class="label-text">Listing Name</span>
                                </label>
                                <label class="input input-bordered flex items-center gap-2">
                                    <svg class="h-[1em] opacity-50" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                                        <g stroke-linejoin="round" stroke-linecap="round" stroke-width="2.5" fill="none" stroke="currentColor">
                                        <circle cx="11" cy="11" r="8"></circle>
                                        <path d="m21 21-4.3-4.3"></path>
                                        </g>
                                    </svg>
                                    <input
                                        type="search"
                                        class="grow"
                                        placeholder="Listing name"
                                        on:input=move |ev| set_name.set(Some(event_target_value(&ev)))
                                        prop:value=move || name.get().unwrap_or_default()
                                    />
                                </label>
                            </div>

                            <div class="form-control">
                                <label class="label cursor-pointer">
                                    <input type="submit" value="Search" class="btn btn-primary" />
                                </label>
                            </div>
                        </div>

                        <details class="collapse bg-base-100 border border-base-300 collapse-plus">
                            <summary class="collapse-title font-semibold">Additional filters</summary>
                            <div class="collapse-content text-sm space-y-4">
                                <div class="form-control w-full max-w-xs">
                                    <fieldset class="fieldset bg-base-100 border-base-300 rounded-box w-64 border p-4">
                                        <legend class="fieldset-legend">Property Type</legend>
                                        <ul>
                                            <li>
                                                <label class="label">
                                                    <input
                                                        type="checkbox"
                                                        class="checkbox"
                                                        on:change=move |_| toggle_structure("Apartment".to_string())
                                                        prop:checked=move || selected_structures.get().contains("Apartment")
                                                />
                                                Apartment
                                                </label>
                                            </li>
                                            <li>
                                                <label class="label">
                                                    <input
                                                        type="checkbox"
                                                        class="checkbox"
                                                        on:change=move |_| toggle_structure("Townhouse".to_string())
                                                        prop:checked=move || selected_structures.get().contains("Townhouse")
                                                />
                                                Townhouse
                                                </label>
                                            </li>
                                            <li>
                                                <label class="label">
                                                    <input
                                                        type="checkbox"
                                                        class="checkbox"
                                                        on:change=move |_| toggle_structure("Studio".to_string())
                                                        prop:checked=move || selected_structures.get().contains("Studio")
                                                />
                                                Studio
                                                </label>
                                            </li>
                                            <li>
                                                <label class="label">
                                                    <input
                                                        type="checkbox"
                                                        class="checkbox"
                                                        on:change=move |_| toggle_structure("House".to_string())
                                                        prop:checked=move || selected_structures.get().contains("House")
                                                />
                                                House
                                                </label>
                                            </li>
                                            <li>
                                                <label class="label">
                                                    <input
                                                        type="checkbox"
                                                        class="checkbox"
                                                        on:change=move |_| toggle_structure("Villa".to_string())
                                                        prop:checked=move || selected_structures.get().contains("Villa")
                                                />
                                                Villa
                                                </label>
                                            </li>
                                        </ul>
                                    </fieldset>
                                </div>

                                <div class="form-control w-full max-w-xs">
                                    <label class="label">
                                        <span class="label-text">Max Price: <span id="price-val">{move || max_price.get().unwrap_or(0.0)}</span></span>
                                    </label>
                                    <input
                                        type="range"
                                        min="0"
                                        max="1000"
                                        step="10"
                                        class="range range-primary"
                                        on:input=move |ev| {
                                            if let Ok(val) = event_target_value(&ev).parse::<f64>() {
                                                set_max_price.set(Some(val));
                                            }
                                        }
                                        prop:value=move || max_price.get().unwrap_or(0.0)
                                    />
                                </div>
                            </div>
                        </details>
                    </form>
                </div>
                </div>

                <div class="space-y-4">
                    <For
                        each=move || listings.get()
                        key=|listing| listing.id
                        children=move |listing| {
                            view! {
                                <div class="card bg-base-100 shadow-sm flex flex-row">
                                    <figure class="w-48 h-48 flex-none">
                                        <img
                                        class="h-full w-full object-cover"
                                        src="https://img.daisyui.com/images/stock/photo-1635805737707-575885ab0820.webp"
                                        alt="Listing Image" />
                                    </figure>
                                    <div class="card-body">
                                        <h2 class="card-title">{listing.name}</h2>
                                        <p class="text-sm text-gray-500">
                                            "Owner: " {listing.owner_name.unwrap_or_else(|| "Unknown".to_string())}
                                        </p>
                                        <p class="text-sm">{listing.description.unwrap_or_default()}</p>
                                        <div class="card-actions justify-end">
                                            <span class="badge badge-outline">{listing.listing_structure}</span>
                                            <span class="badge badge-ghost">
                                                {listing.price_per_night.map(|p| format!("${}", p)).unwrap_or_default()}
                                            </span>
                                            <button class="btn btn-primary btn-sm">View</button>
                                        </div>
                                    </div>
                                </div>
                            }
                        }
                    />
                    {move || {
                        if listing_search.pending().get() {
                            view! { <span class="loading loading-spinner loading-md">Loading...</span> }.into_any()
                        } else if listings.get().is_empty() && listing_search.input().with(|i| i.is_some()) {
                            view! {
                                <div class="alert alert-info">
                                    <span>"No listings found match your criteria."</span>
                                </div>
                            }.into_any()
                        } else {
                            ().into_any()
                        }
                    }}
                </div>
            </div>
        </div>
        </RequireAuth>

    }
}
