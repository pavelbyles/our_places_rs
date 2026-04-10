use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use num_format::{Locale, ToFormattedString};
use rust_decimal::prelude::ToPrimitive;
use super::booking_card::BookingCard;
use web_app_common::listings::get_listing_by_id_server;

#[component]
#[allow(non_snake_case)]
pub fn ListingDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id").unwrap_or_default());

    let listing_resource = Resource::new(
        move || id(),
        |id_str| async move {
            if id_str.is_empty() {
                return Err(ServerFnError::new("No ID provided"));
            }
            get_listing_by_id_server(id_str).await
        },
    );

    view! {
        <Suspense fallback=move || view! { <div class="p-10 text-center">"Loading listing..."</div> }>
            {move || {
                listing_resource.get().map(|res| match res {
                    Ok(details) => {
                        let listing = details.listing;
                        let images = details.images;

                        let carousel_content = if images.is_empty() {
                            view! {
                                <div class="carousel w-full">
                                    <div class="carousel-item relative w-full h-[300px] md:h-[500px]">
                                        <img
                                            src="https://img.daisyui.com/images/stock/photo-1625726411847-8cbb60cc71e6.webp"
                                            class="w-full object-cover"
                                            alt="Placeholder"
                                        />
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            let images_len = images.len();
                            let listing_name_for_img = listing.name.clone();

                            view! {
                                <div class="carousel w-full">
                                    <For
                                        each=move || images.clone().into_iter().enumerate()
                                        key=|(i, _)| *i
                                        children=move |(i, img)| {
                                            let prev_i = if i == 0 { images_len - 1 } else { i - 1 };
                                            let next_i = if i == images_len - 1 { 0 } else { i + 1 };
                                            let slide_id = format!("slide{i}");
                                            let prev_slide = format!("#slide{prev_i}");
                                            let next_slide = format!("#slide{next_i}");

                                            let mobile_url = &img.url;
                                            let tablet_url = &img.url;
                                            let desktop_url = &img.url;

                                            view! {
                                                <div id=slide_id class="carousel-item relative w-full h-[300px] md:h-[500px]">
                                                    <img
                                                        srcset=format!("{mobile_url} 640w, {tablet_url} 1024w, {desktop_url} 1920w")
                                                        sizes="(max-width: 640px) 100vw, (max-width: 1024px) 50vw, 33vw"
                                                        src=tablet_url.clone()
                                                        alt=listing_name_for_img.clone()
                                                        class="w-full object-cover"
                                                    />
                                                    {
                                                        if images_len > 1 {
                                                            view! {
                                                                <div class="absolute left-5 right-5 top-1/2 flex -translate-y-1/2 transform justify-between">
                                                                    <a href=prev_slide class="btn btn-circle btn-sm md:btn-md bg-base-100/50 hover:bg-base-100">"❮"</a>
                                                                    <a href=next_slide class="btn btn-circle btn-sm md:btn-md bg-base-100/50 hover:bg-base-100">"❯"</a>
                                                                </div>
                                                            }.into_any()
                                                        } else {
                                                            view! { <div></div> }.into_any()
                                                        }
                                                    }
                                                </div>
                                            }
                                        }
                                    />
                                </div>
                            }.into_any()
                        };

                        view! {
                            <div class="w-full max-w-6xl mx-auto mt-4 mb-20 px-4 flex flex-col gap-8">
                                {carousel_content}

                                <div class="grid grid-cols-1 lg:grid-cols-3 gap-12">
                                    // Left Column: Main Content
                                    <div class="lg:col-span-2 flex flex-col gap-8">
                                        <div>
                                            <h1 class="text-4xl font-bold text-base-content">{listing.name.clone()}</h1>
                                            <p class="text-xl text-base-content/70 mt-2 flex items-center gap-2">
                                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5 inline-block shrink-0">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M15 10.5a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 10.5c0 7.142-7.5 11.25-7.5 11.25S4.5 17.642 4.5 10.5a7.5 7.5 0 1 1 15 0Z" />
                                                </svg>
                                                {listing.city.clone().unwrap_or_default()} {if listing.city.is_some() { ", " } else { "" }} {listing.country.clone()}
                                            </p>
                                        </div>

                                        <div class="hidden lg:block h-px bg-base-200"></div>

                                        <div class="flex flex-col gap-4">
                                            <h2 class="text-2xl font-semibold text-base-content">"About this place"</h2>
                                            <p class="whitespace-pre-line text-lg text-base-content/80 leading-relaxed">{listing.description.clone().unwrap_or_default()}</p>
                                        </div>

                                        <div class="flex flex-wrap gap-6 text-base-content/80 text-lg border-y border-base-200 py-6">
                                            <div class="flex items-center gap-2">
                                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="size-6 text-primary">
                                                  <path stroke-linecap="round" stroke-linejoin="round" d="M15 19.128a9.38 9.38 0 0 0 2.625.372 9.337 9.337 0 0 0 4.121-.952 4.125 4.125 0 0 0-7.533-2.493M15 19.128v-.003c0-1.113-.285-2.16-.786-3.07M15 19.128v.106A12.318 12.318 0 0 1 8.624 21c-2.331 0-4.512-.645-6.374-1.766l-.001-.109a6.375 6.375 0 0 1 11.964-3.07M12 6.375a3.375 3.375 0 1 1-6.75 0 3.375 3.375 0 0 1 6.75 0Zm8.25 2.25a2.625 2.625 0 1 1-5.25 0 2.625 2.625 0 0 1 5.25 0Z" />
                                                </svg>
                                                <span>{listing.max_guests} " Guests"</span>
                                            </div>
                                            <div class="flex items-center gap-2">
                                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="size-6 text-primary">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M2.25 12l8.954-8.955c.44-.439 1.152-.439 1.591 0L21.75 12M4.5 9.75v10.125c0 .621.504 1.125 1.125 1.125H9.75v-4.875c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125V21h4.125c.621 0 1.125-.504 1.125-1.125V9.75M8.25 21h8.25" />
                                                </svg>
                                                <span>{listing.bedrooms} " Bedrooms"</span>
                                            </div>
                                            <div class="flex items-center gap-2">
                                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="size-6 text-primary">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M3 8.25V18a2.25 2.25 0 0 0 2.25 2.25h13.5A2.25 2.25 0 0 0 21 18V8.25m-18 0V6a2.25 2.25 0 0 1 2.25-2.25h13.5A2.25 2.25 0 0 1 21 6v2.25m-18 0h18M5.25 6h.008v.008H5.25V6ZM7.5 6h.008v.008H7.5V6Zm2.25 0h.008v.008H9.75V6Z" />
                                                </svg>
                                                <span>{listing.full_bathrooms} " Bathrooms"</span>
                                            </div>
                                        </div>

                                        {
                                            if listing.listing_details.is_some() {
                                                view! {
                                                    <div class="flex flex-col gap-4">
                                                        <h3 class="text-2xl font-semibold text-base-content">"Amenities"</h3>
                                                        <div class="grid grid-cols-2 gap-4">
                                                            <For
                                                                each=move || {
                                                                    listing.listing_details.as_ref()
                                                                        .and_then(|v| v.as_object())
                                                                        .map(|obj| {
                                                                            obj.iter().map(|(k, _)| k.to_string()).enumerate().collect::<Vec<_>>()
                                                                        })
                                                                        .unwrap_or_default()
                                                                }
                                                                key=|detail| detail.0
                                                                children=|detail| {
                                                                    let key = detail.1;
                                                                    view! {
                                                                        <div class="flex items-center gap-2 text-lg text-base-content/80">
                                                                            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-5 h-5 text-success">
                                                                                <path stroke-linecap="round" stroke-linejoin="round" d="m4.5 12.75 6 6 9-13.5" />
                                                                            </svg>
                                                                            <span>{key}</span>
                                                                        </div>
                                                                    }.into_any()
                                                                }
                                                            />
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <div></div> }.into_any()
                                            }
                                        }
                                    </div>

                                    // Right Column: Booking Card
                                    <div class="lg:col-span-1">
                                        <div class="sticky top-8 flex flex-col gap-4">
                                            <BookingCard id_or_slug=id() listing=listing.clone() />
                                        </div>
                                    </div>
                                </div>
                            </div>
                        }.into_any()
                    }
                    Err(e) => view! {
                        <div class="p-10 text-center text-error border border-error bg-error/10 rounded-lg max-w-lg mx-auto">
                            <h2 class="text-xl font-bold mb-2">"Error loading listing details"</h2>
                            <p>{e.to_string()}</p>
                        </div>
                    }.into_any()
                })
            }}
        </Suspense>
    }
}
