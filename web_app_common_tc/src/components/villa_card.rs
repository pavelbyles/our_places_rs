use topcoat::{
    Result,
    view::{component, view},
};

#[allow(clippy::too_many_arguments)]
#[component]
pub async fn villa_card(
    id: String,
    title: String,
    image_url: String,
    price: String,
    currency: String,
    country: String,
    city: Option<String>,
    max_guests: i32,
    bedrooms: i32,
    full_bathrooms: i32,
    rating: Option<f64>,
    review_count: Option<i64>,
) -> Result {
    let location_text = if let Some(ref c) = city {
        format!("{c}, {country}")
    } else {
        country.clone()
    };
    let specs_text = format!("{max_guests} Guests · {bedrooms} Beds · {full_bathrooms} Baths");
    let details_link = format!("/listings/{}", id);
    let title_display = title.clone();
    let image_src = image_url.clone();
    let image_alt = title.clone();
    let price_display = format!("{currency} {price}");
    let rating_val = rating.unwrap_or(4.95);
    let reviews_val = review_count.unwrap_or(112);

    view! {
        <div class="card bg-base-100 shadow-lg border border-base-200/80 hover:shadow-2xl transition-all duration-300 rounded-2xl overflow-hidden group flex flex-col justify-between">
            <figure class="relative overflow-hidden aspect-[16/10] bg-base-200">
                <img
                    src=(image_src)
                    alt=(image_alt)
                    class="object-cover h-full w-full group-hover:scale-105 transition-transform duration-500"
                    loading="lazy"
                />
                // Price Pill Overlay
                <div class="absolute top-3 right-3 bg-neutral/85 text-neutral-content backdrop-blur-md px-3 py-1 rounded-lg text-sm font-bold shadow-md">
                    (price_display)<span class="text-xs font-normal opacity-75">"/night"</span>
                </div>
            </figure>

            <div class="p-5 space-y-3 flex-1 flex flex-col justify-between">
                <div class="space-y-1.5">
                    <div class="flex justify-between items-baseline gap-2">
                        <a href=(details_link.clone()) class="hover:text-primary transition-colors">
                            <h3 class="font-serif font-bold text-lg md:text-xl tracking-tight text-base-content group-hover:text-primary transition-colors">
                                (title_display)
                            </h3>
                        </a>
                        <div class="flex items-center gap-1 text-xs font-bold text-amber-500 shrink-0">
                            <span>"★"</span>
                            <span>(format!("{:.2}", rating_val))</span>
                            <span class="text-base-content/50 font-normal text-[11px]">(format!("({} reviews)", reviews_val))</span>
                        </div>
                    </div>
                    <p class="text-xs text-base-content/60 font-medium flex items-center gap-1">
                        <span>"📍"</span> (location_text) <span class="opacity-40">"•"</span> (specs_text)
                    </p>
                </div>

                // Curated Amenity Pills
                <div class="flex flex-wrap gap-1.5 pt-1">
                    <span class="badge badge-sm badge-ghost text-[10px] font-semibold">"Infinity Pool"</span>
                    <span class="badge badge-sm badge-ghost text-[10px] font-semibold">"Private Beach"</span>
                    <span class="badge badge-sm badge-ghost text-[10px] font-semibold">"Staffed"</span>
                </div>

                <div class="pt-3 border-t border-base-200/60 flex justify-end items-center">
                    <a class="btn btn-outline btn-primary btn-sm rounded-xl px-4 font-semibold tracking-wide" href=(details_link)>
                        "View Villa"
                    </a>
                </div>
            </div>
        </div>
    }
}
