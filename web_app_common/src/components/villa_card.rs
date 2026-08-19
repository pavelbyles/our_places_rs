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
    #[prop(into)] currency: String,
    #[prop(into)] rating: Option<f64>,
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
                <div class="flex justify-between items-start">
                    <a href=format!("/listing/{}", id) class="hover:underline">
                        <h2 class="card-title text-2xl">{title}</h2>
                    </a>
                    {
                        if let Some(r) = rating {
                            view! {
                                <div class="badge badge-lg badge-primary gap-1 font-bold">
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                                        <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                                    </svg>
                                    {format!("{:.1}", r)}
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="badge badge-lg badge-outline gap-1 font-semibold text-base-content/60">
                                    "New"
                                </div>
                            }.into_any()
                        }
                    }
                </div>
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
                        {currency} " " {price} <span class="text-sm font-normal opacity-70">"/night"</span>
                    </div>

                    <a class="btn btn-primary" href=format!("/listing/{}", id)>"View Details"</a>
                </div>
            </div>
        </div>
    }
}
