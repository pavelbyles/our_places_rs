use leptos::prelude::*;

#[component]
pub fn VillaCard(
    #[prop(into)] title: String,
    #[prop(into)] image_url: String,
    #[prop(into)] price: String,
    #[prop(into)] max_guests: i32,
    #[prop(into)] bedrooms: i32,
    #[prop(into)] full_bathrooms: i32,
    #[prop(into)] country: String,
    #[prop(into)] city: Option<String>,
    #[prop(into)] id: String,
) -> impl IntoView {
    let navigate = leptos_router::hooks::use_navigate();

    let navigate_to_listing = move |_| {
        navigate(&format!("/listing/{}", id), Default::default());
    };
    view! {
        <div class="card md:card-side bg-base-100 shadow-sm border border-base-200">
            <figure class="md:w-2/5">
                <img
                    src=image_url
                    alt=title.clone()
                    class="object-cover h-full w-full max-h-64 md:max-h-none"
                />
            </figure>
            <div class="card-body md:w-3/5">
                <h2 class="card-title text-2xl">{title}</h2>
                <p class="text-base-content/70 flex items-center gap-1">
                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4 inline-block shrink-0">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M15 10.5a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
                        <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 10.5c0 7.142-7.5 11.25-7.5 11.25S4.5 17.642 4.5 10.5a7.5 7.5 0 1 1 15 0Z" />
                    </svg>
                    {
                        if let Some(c) = city {
                            format!("{country}, {c}")
                        } else {
                            country
                        }
                    }
                </p>
                <p class="text-base-content/70"> {max_guests} " guest(s)" | {bedrooms} " bedroom(s)" | {full_bathrooms} " bathroom(s)" </p>
                <div class="card-actions justify-between items-center mt-4">
                    <div class="text-xl font-bold">
                        "$" {price} <span class="text-sm font-normal opacity-70">"/night"</span>
                    </div>

                    // TODO: `on_click` closure as a prop here to handle routing dynamically
                    <button class="btn btn-primary" on:click=navigate_to_listing>"View Details"</button>
                </div>
            </div>
        </div>
    }
}
