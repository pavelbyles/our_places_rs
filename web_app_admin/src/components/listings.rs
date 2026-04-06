use crate::components::protected::RequireAuth;
use leptos::ev::SubmitEvent;
use leptos::form::ActionForm;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use web_app_common::listings::{ListingSearchServer};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateListingParams {
    pub name: String,
    pub user_id: String,
    pub description: Option<String>,
    pub listing_structure: String,
    pub country: String,
    pub price_per_night: Option<f64>,
    pub weekly_discount_percentage: Option<f64>,
    pub monthly_discount_percentage: Option<f64>,
}

#[server]
pub async fn create_listing_server(params: CreateListingParams) -> Result<String, ServerFnError> {
    use uuid::Uuid;
    let user_id = Uuid::parse_str(&params.user_id)
        .map_err(|e| ServerFnError::new(format!("Invalid UUID: {}", e)))?;

    use rust_decimal::prelude::FromPrimitive;
    let request = common::models::NewListingRequest {
        name: params.name,
        user_id,
        description: params.description,
        listing_structure: params.listing_structure,
        country: params.country,
        price_per_night: params.price_per_night.and_then(rust_decimal::Decimal::from_f64),
        weekly_discount_percentage: params.weekly_discount_percentage.and_then(rust_decimal::Decimal::from_f64),
        monthly_discount_percentage: params.monthly_discount_percentage.and_then(rust_decimal::Decimal::from_f64),
    };

    let api_url = crate::api_client::listing_api_url();
    let res = crate::api_client::get_client()
        .post(&format!("{}/api/v1/listings", api_url), &api_url, &request)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if res.status().is_success() {
        let listing: common::models::ListingResponse = res
            .json()
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(listing.id.to_string())
    } else {
        Err(ServerFnError::new(format!(
            "Failed to create listing: {}",
            res.status()
        )))
    }
}

#[server]
pub async fn presign_images_server(
    listing_id: String,
    images: Vec<common::models::PendingImageMetadata>,
) -> Result<Vec<common::models::ImagePresignResponse>, ServerFnError> {
    let api_url = crate::api_client::listing_api_url();
    let request = common::models::ImagePresignRequest { images };

    let res = crate::api_client::get_client()
        .post(
            &format!("{}/api/v1/listings/{}/images/presign", api_url, listing_id),
            &api_url,
            &request,
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if res.status().is_success() {
        let presign_res: Vec<common::models::ImagePresignResponse> = res
            .json()
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(presign_res)
    } else {
        Err(ServerFnError::new(format!(
            "Failed to presign images: {}",
            res.status()
        )))
    }
}

#[component]
pub fn ListingsPage() -> impl IntoView {
    let listing_search = ServerAction::<ListingSearchServer>::new();
    let create_listing = ServerAction::<CreateListingServer>::new();
    let (name, set_name) = signal(None::<String>);
    let (owner_email, set_owner_email) = signal(None::<String>);
    let (max_price, set_max_price) = signal(Some(0.0));
    let (selected_structures, set_selected_structures) = signal(HashSet::<String>::new());

    let (owner_email_input, set_owner_email_input) = signal(String::new());
    let (owner_id_validated, set_owner_id_validated) = signal(None::<String>);
    let (owner_id_error, set_owner_id_error) = signal(false);

    let (uploading_images, set_uploading_images) = signal(false);

    let timeout_handle = StoredValue::new(None::<TimeoutHandle>);

    // 1. Create listing
    // 2. Get number of files being uploaded
    // 3. For each file - add metadata to vec
    // 4. Call presign fn
    // 5. Get back urls
    // 6. For each response we get back create and make a request to url
    Effect::new(move |_| {
        if let Some(Ok(listing_id)) = create_listing.value().get() {
            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            if let Some(document) = window.document() {
                if let Some(element) = document.get_element_by_id("file-upload") {
                    use wasm_bindgen::JsCast;
                    if let Ok(input) = element.dyn_into::<web_sys::HtmlInputElement>() {
                        // Get loaded file(s) info
                        if let Some(files) = input.files() {
                            let count = files.length();
                            if count > 0 {
                                set_uploading_images.set(true);
                                let mut metadata = Vec::new();
                                // Store a mapping of client_file_id -> actual file index
                                let mut local_file_map = std::collections::HashMap::new();

                                // Get and store metadata
                                for i in 0..count {
                                    if let Some(file) = files.item(i) {
                                        let client_file_id = uuid::Uuid::new_v4().to_string();
                                        local_file_map.insert(client_file_id.clone(), i);

                                        metadata.push(common::models::PendingImageMetadata {
                                            client_file_id,
                                            content_type: file.type_(),
                                            size_bytes: file.size() as u64,
                                            display_order: i as i32,
                                        });
                                    }
                                }

                                // Get presigned URL's from backend
                                spawn_local(async move {
                                    match presign_images_server(listing_id.clone(), metadata).await
                                    {
                                        Ok(responses) => {
                                            let mut upload_futures = Vec::new();
                                            for res in responses {
                                                if let Some(&file_idx) =
                                                    local_file_map.get(&res.client_file_id)
                                                {
                                                    if let Some(file) = files.item(file_idx) {
                                                        let url = &res.upload_url;
                                                        let opts = web_sys::RequestInit::new();
                                                        opts.set_method("PUT");
                                                        let js_val: wasm_bindgen::JsValue =
                                                            file.into();
                                                        opts.set_body(&js_val);
                                                        // Upload file to GCS
                                                        if let Ok(request) =
                                                            web_sys::Request::new_with_str_and_init(
                                                                url, &opts,
                                                            )
                                                        {
                                                            let fut = wasm_bindgen_futures::JsFuture::from(
                                                                window.fetch_with_request(&request), 
                                                            );
                                                            upload_futures.push(fut);
                                                        }
                                                    }
                                                }
                                            }
                                            futures::future::join_all(upload_futures).await;
                                        }
                                        Err(e) => {
                                            leptos::logging::error!(
                                                "Failed to get presigned URLs: {:?}",
                                                e
                                            );
                                        }
                                    }
                                    set_uploading_images.set(false);
                                });
                            }
                        }
                    }
                }
            }
        }
    });

    let on_email_input = move |ev| {
        let val = event_target_value(&ev);
        set_owner_email_input.set(val.clone());
        set_owner_id_validated.set(None);
        set_owner_id_error.set(false);

        timeout_handle.update_value(|h: &mut Option<TimeoutHandle>| {
            if let Some(handle) = h.take() {
                handle.clear();
            }
        });

        if val.is_empty() {
            return;
        }

        let handle = set_timeout_with_handle(
            move || {
                spawn_local(async move {
                    match crate::components::user::get_user_server(val).await {
                        Ok(user) => {
                            set_owner_id_validated.set(Some(user.id.to_string()));
                            set_owner_id_error.set(false);
                        }
                        Err(_) => {
                            set_owner_id_validated.set(None);
                            set_owner_id_error.set(true);
                        }
                    }
                });
            },
            std::time::Duration::from_secs(2),
        )
        .ok();

        timeout_handle.set_value(handle);
    };

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
                <div class="card bg-base-300 rounded-box grid h-32 grow place-items-center">
                    <h2>Search Listings</h2>
                    <div class="flex w-full flex-col lg:flex-row">
                        <div class="card bg-base-300 rounded-box grid grow p-4">
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
                                </form>

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
                                                        src={listing.primary_image_url.clone().unwrap_or_else(|| "https://img.daisyui.com/images/stock/photo-1635805737707-575885ab0820.webp".to_string())}
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
                            </div>
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
                <div class="divider lg:divider-horizontal">-</div>
                <div class="card bg-base-300 rounded-box grid grow place-items-center p-4">
                    <h2>Add New Listing</h2>
                    <ActionForm action={create_listing} attr:class="form-control w-full max-w-xs space-y-4">
                        <div>
                            <label for="listing_name" class="label">
                                <span class="label-text">Listing Name</span>
                            </label>
                            <input type="text" name="params[name]" placeholder="Listing Name" class="input input-bordered w-full max-w-xs" required />
                        </div>
                        <div>
                            <label for="owner_email" class="label">
                                <span class="label-text">Owner Email</span>
                            </label>
                            <label
                                class=move || {
                                    if owner_id_validated.get().is_some() {
                                        "input input-bordered flex items-center gap-2 w-full max-w-xs input-success"
                                    } else if owner_id_error.get() {
                                        "input input-bordered flex items-center gap-2 w-full max-w-xs input-error"
                                    } else {
                                        "input input-bordered flex items-center gap-2 w-full max-w-xs"
                                    }
                                }
                            >
                                <input
                                    type="text"
                                    placeholder="Owner Email"
                                    class="grow"
                                    on:input=on_email_input
                                    prop:value=move || owner_email_input.get()
                                />
                                {move || {
                                    if owner_id_validated.get().is_some() {
                                        view! {
                                            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="fill-green-500 size-4">
                                                <path fill-rule="evenodd" d="M12.416 3.376a.75.75 0 0 1 .208 1.04l-5 7.5a.75.75 0 0 1-1.154.114l-3-3a.75.75 0 0 1 1.06-1.06l2.353 2.353 4.493-6.74a.75.75 0 0 1 1.04-.207Z" clip-rule="evenodd" />
                                            </svg>
                                        }.into_any()
                                    } else if owner_id_error.get() {
                                        view! {
                                            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="fill-red-500 size-4">
                                                <path d="M5.28 4.22a.75.75 0 0 0-1.06 1.06L6.94 8l-2.72 2.72a.75.75 0 1 0 1.06 1.06L8 9.06l2.72 2.72a.75.75 0 1 0 1.06-1.06L9.06 8l2.72-2.72a.75.75 0 0 0-1.06-1.06L8 6.94 5.28 4.22Z" />
                                            </svg>
                                        }.into_any()
                                    } else {
                                        ().into_any()
                                    }
                                }}
                            </label>
                            <input type="hidden" name="params[user_id]" value=move || owner_id_validated.get().unwrap_or_default() />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Description</span>
                            </label>
                            <textarea name="params[description]" placeholder="Description" class="textarea textarea-bordered h-24 w-full max-w-xs"></textarea>
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Structure Type</span>
                            </label>
                            <select name="params[listing_structure]" class="select select-bordered w-full max-w-xs">
                                <option disabled selected>Select property type</option>
                                <option value="Apartment">Apartment</option>
                                <option value="House">House</option>
                                <option value="Studio">Studio</option>
                                <option value="Townhouse">Townhouse</option>
                                <option value="Villa">Villa</option>
                            </select>
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Country</span>
                            </label>
                            <input type="text" name="params[country]" placeholder="Country" class="input input-bordered w-full max-w-xs" required />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Price Per Night ($)</span>
                            </label>
                            <input type="number" step="0.01" min="0" name="params[price_per_night]" placeholder="0.00" class="input input-bordered w-full max-w-xs" />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Weekly Discount (%)</span>
                            </label>
                            <input type="number" step="0.1" min="0" max="100" name="params[weekly_discount_percentage]" placeholder="0.0" class="input input-bordered w-full max-w-xs" />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Monthly Discount (%)</span>
                            </label>
                            <input type="number" step="0.1" min="0" max="100" name="params[monthly_discount_percentage]" placeholder="0.0" class="input input-bordered w-full max-w-xs" />
                        </div>
                        <div>
                            <label class="label">
                                <span class="label-text">Upload images (max 10)</span>
                            </label>
                            <input type="file" id="file-upload" multiple />
                        </div>

                        <button type="submit" class="btn btn-primary" disabled=move || create_listing.pending().get() || owner_id_validated.get().is_none() || uploading_images.get()>
                            {move || {
                                if create_listing.pending().get() {
                                    "Creating..."
                                } else if uploading_images.get() {
                                    "Uploading Images..."
                                } else {
                                    "Create Listing"
                                }
                            }}
                        </button>

                        {move || create_listing.value().get().map(|v| match v {
                            Ok(_) => view! { <div class="alert alert-success mt-4"><span>"Listing created successfully"</span></div> }.into_any(),
                            Err(e) => view! { <div class="alert alert-error mt-4"><span>{e.to_string()}</span></div> }.into_any(),
                        })}
                    </ActionForm>
                </div>
            </div>
        </RequireAuth>
    }
}
