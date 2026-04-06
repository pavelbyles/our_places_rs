use leptos::prelude::*;

#[component]
pub fn VillaCard(
    #[prop(into)] title: String,
    #[prop(into)] description: String,
    #[prop(into)] image_url: String,
    #[prop(into)] price: String,
) -> impl IntoView {
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
                <p class="text-base-content/70">{description}</p>
                <div class="card-actions justify-between items-center mt-4">
                    <div class="text-xl font-bold">
                        "$" {price} <span class="text-sm font-normal opacity-70">"/night"</span>
                    </div>

                    // TODO: `on_click` closure as a prop here to handle routing dynamically
                    <button class="btn btn-primary">"View Details"</button>
                </div>
            </div>
        </div>
    }
}
