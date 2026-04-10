use leptos::prelude::*;
use num_format::{Locale, ToFormattedString};
use rust_decimal::prelude::ToPrimitive;
use web_app_common::listings::get_listing_by_id_server;
use common::models::{ListingResponse};

#[component]
#[allow(non_snake_case)]
pub fn BookingCard(
    #[prop(into)]
    id_or_slug: String,
    #[prop(optional)]
    listing: Option<ListingResponse>
) -> impl IntoView {
    
    let listing_resource = Resource::new(
        move || (id_or_slug.clone(), listing.clone()),
        |(id, opt_listing)| async move {
            if let Some(l) = opt_listing {
                Ok(l)
            } else {
                if id.is_empty() {
                    return Err(ServerFnError::new("No ID provided"));
                }
                get_listing_by_id_server(id).await.map(|details| details.listing)
            }
        },
    );

    view! {
        <div class="card bg-base-100 w-full shadow-xl border border-base-200">
            <Suspense fallback=move || view! { <div class="p-4 text-center">"Loading price..."</div> }>
                {move || {
                    listing_resource.get().map(|res| match res {
                        Ok(listing) => {
                            view! {
                                <div class="card-body gap-6">
                                    <div class="flex justify-between items-end">
                                        <div class="text-3xl font-bold text-primary">
                                            "$" {listing.price_per_night.map(|p| p.to_i64().unwrap().to_formatted_string(&Locale::en))
                                            .unwrap_or_else(|| "0.00".to_string())}
                                            <span class="text-lg font-normal text-base-content/70">" / night"</span>
                                        </div>
                                        <div class="text-sm font-medium flex items-center gap-1">
                                            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-4 h-4 text-warning">
                                                <path fill-rule="evenodd" d="M10.788 3.21c.448-1.077 1.976-1.077 2.424 0l2.082 5.007 5.404.433c1.164.093 1.636 1.545.749 2.305l-4.117 3.527 1.257 5.273c.271 1.136-.964 2.033-1.96 1.425L12 18.354 7.373 21.18c-.996.608-2.231-.29-1.96-1.425l1.257-5.273-4.117-3.527c-.887-.76-.415-2.212.749-2.305l5.404-.433 2.082-5.006z" clip-rule="evenodd" />
                                            </svg>
                                            {listing.overall_rating.map(|r| format!("{:.1}", r)).unwrap_or_else(|| "New".to_string())}
                                        </div>
                                    </div>

                                    <div class="flex flex-col border border-base-300 rounded-xl overflow-hidden mt-4">
                                        <div class="grid grid-cols-2 border-b border-base-300">
                                            <div class="p-3 border-r border-base-300 hover:bg-base-200/50 transition-colors cursor-pointer">
                                                <label class="block text-[10px] font-bold uppercase text-base-content/60">"Check-in"</label>
                                                <input type="date" class="w-full bg-transparent text-sm focus:outline-none cursor-pointer mt-1" />
                                            </div>
                                            <div class="p-3 hover:bg-base-200/50 transition-colors cursor-pointer">
                                                <label class="block text-[10px] font-bold uppercase text-base-content/60">"Check-out"</label>
                                                <input type="date" class="w-full bg-transparent text-sm focus:outline-none cursor-pointer mt-1" />
                                            </div>
                                        </div>
                                        <div class="p-3 hover:bg-base-200/50 transition-colors cursor-pointer">
                                            <label class="block text-[10px] font-bold uppercase text-base-content/60">"# of Guests"</label>
                                            <select class="w-full bg-transparent text-sm focus:outline-none cursor-pointer mt-1 appearance-none">
                                                {
                                                    (1..=listing.max_guests).map(|n| {
                                                        view! { <option value=n>{n} {if n == 1 { " guest" } else { " guests" }}</option> }
                                                    }).collect_view()
                                                }
                                            </select>
                                        </div>
                                    </div>

                                    <div class="flex flex-col gap-2">
                                        <button class="btn btn-primary btn-lg w-full">"Book Now"</button>
                                        <p class="text-center text-sm text-base-content/60">"You won't be charged yet"</p>
                                    </div>

                                    <div class="divider my-0"></div>

                                    <div class="flex flex-col gap-3">
                                        <div class="flex justify-between text-lg">
                                            <span class="underline">"Price per night"</span>
                                            <span>"$" {listing.price_per_night.map(|p| p.to_i64().unwrap().to_formatted_string(&Locale::en)).unwrap_or_else(|| "0.00".to_string())}</span>
                                        </div>
                                        <div class="flex justify-between text-lg font-bold mt-2 pt-4 border-t border-base-200">
                                            <span>"Total"</span>
                                            <span>"$" {listing.price_per_night.map(|p| p.to_i64().unwrap().to_formatted_string(&Locale::en)).unwrap_or_else(|| "0.00".to_string())}</span>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        }
                        Err(e) => view! {
                            <div class="card-body p-4 text-error">
                                <p>"Error loading booking info"</p>
                                <p class="text-xs">{e.to_string()}</p>
                            </div>
                        }.into_any()
                    })
                }}
            </Suspense>
        </div>
    }
}
