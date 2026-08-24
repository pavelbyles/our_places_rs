use super::booking_card::BookingCard;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use web_app_common::listings::get_listing_by_id_server;

#[component]
#[allow(non_snake_case)]
pub fn ListingDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id").unwrap_or_default());

    let auth_context = use_context::<crate::app::AuthContext>();
    let currency = move || {
        auth_context
            .as_ref()
            .and_then(|c| c.user.get())
            .and_then(|res| res.ok().flatten())
            .map(|u| u.default_currency)
    };

    let listing_resource = Resource::new(
        move || (id(), currency()),
        |(id_str, curr)| async move {
            if id_str.is_empty() {
                return Err(ServerFnError::new("No ID provided"));
            }
            get_listing_by_id_server(id_str, curr).await
        },
    );

    view! {
        <Suspense fallback=move || view! { <div class="p-10 text-center">"Loading listing..."</div> }>
            {move || {
                listing_resource.get().map(|res| match res {
                    Ok(details) => {
                        let listing = details.listing;
                        let images = details.images;
                        let host_name = details.host_name;

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

                                        {
                                            let hn = host_name.clone();
                                            view! {
                                                {move || {
                                                    hn.as_ref().map(|name| {
                                                        let avatar_url = format!("https://ui-avatars.com/api/?name={}&background=random&size=80&rounded=true", name);
                                                        view! {
                                                            <div class="flex items-center gap-3 py-4 border-y border-base-200">
                                                                <div class="avatar">
                                                                    <div class="w-10 h-10 rounded-full ring ring-primary/20">
                                                                        <img
                                                                            src=avatar_url
                                                                            alt=format!("Host {}", name)
                                                                            width="40"
                                                                            height="40"
                                                                        />
                                                                    </div>
                                                                </div>
                                                                <span class="text-lg text-base-content/80">"Hosted by " {name.clone()}</span>
                                                            </div>
                                                        }
                                                    })
                                                }}
                                            }
                                        }

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
                                                            {
                                                                let details_for_amenities = listing.listing_details.clone();
                                                                view! {
                                                                    <For
                                                                        each=move || {
                                                                            details_for_amenities.as_ref()
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
                                                                }
                                                            }
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

                                <div class="h-px bg-base-200 my-4"></div>

                                // Reviews Section (Full width below main details)
                                <ListingReviews listing_id=listing.id rating_summary=details.rating_summary />
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

#[component]
fn ListingReviews(
    listing_id: uuid::Uuid,
    rating_summary: Option<common::models::ListingRatingSummary>,
) -> impl IntoView {
    use web_app_common::reviews::get_listing_reviews_server;

    // We only fetch reviews if there's a rating summary indicating there are reviews
    let review_count = rating_summary.as_ref().map(|s| s.review_count).unwrap_or(0);

    let reviews_resource = Resource::new(
        move || listing_id,
        |id| async move { get_listing_reviews_server(id, 1, 10).await },
    );

    view! {
        <div class="flex flex-col gap-8 w-full">
            {
                if let Some(summary) = rating_summary.as_ref() {
                    let overall = summary.overall_rating.unwrap_or(0.0);
                    view! {
                        <div class="flex flex-col gap-6">
                            <div class="flex items-center gap-4">
                                <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8 text-primary" viewBox="0 0 20 20" fill="currentColor">
                                    <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                                </svg>
                                <span class="text-3xl font-bold text-base-content">{format!("{:.2}", overall)}</span>
                                <span class="text-2xl text-base-content/60">"·"</span>
                                <span class="text-2xl font-bold text-base-content">{summary.review_count} " Reviews"</span>
                            </div>

                            // Sub-ratings grid
                            <div class="grid grid-cols-1 md:grid-cols-2 gap-x-12 gap-y-4">
                                <RatingBar label="Cleanliness" value=summary.cleanliness_rating.unwrap_or(0.0) />
                                <RatingBar label="Accuracy" value=summary.accuracy_rating.unwrap_or(0.0) />
                                <RatingBar label="Location" value=summary.location_rating.unwrap_or(0.0) />
                                <RatingBar label="Value" value=summary.value_rating.unwrap_or(0.0) />
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div>
                            <h2 class="text-2xl font-bold text-base-content">"No reviews (yet)"</h2>
                            <p class="text-base-content/60 mt-2">"This host has no reviews for this place."</p>
                        </div>
                    }.into_any()
                }
            }

            {
                if review_count > 0 {
                    view! {
                        <Suspense fallback=move || view! { <div class="loading loading-spinner"></div> }>
                            {move || reviews_resource.get().map(|res| match res {
                                Ok(reviews) => {
                                    view! {
                                        <div class="grid grid-cols-1 md:grid-cols-2 gap-8 mt-6">
                                            <For
                                                each=move || reviews.clone()
                                                key=|review| review.id
                                                children=move |review| {
                                                    view! {
                                                        <div class="flex flex-col gap-4">
                                                            <div class="flex items-center gap-4">
                                                                <div class="avatar">
                                                                    <div class="w-12 h-12 rounded-full bg-base-300 text-base-content flex items-center justify-center font-bold text-xl uppercase">
                                                                        {review.guest_first_name.chars().next().unwrap_or('?').to_string()}
                                                                    </div>
                                                                </div>
                                                                <div class="flex flex-col">
                                                                    <span class="font-bold text-base-content">{review.guest_first_name}</span>
                                                                    <div class="flex items-center gap-1 text-sm text-base-content/70">
                                                                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                                                                            <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                                                                        </svg>
                                                                        <span>{format!("{:.1}", review.overall_rating)}</span>
                                                                    </div>
                                                                </div>
                                                            </div>
                                                            <p class="text-base-content/80 whitespace-pre-line leading-relaxed text-sm lg:text-base">
                                                                {review.public_review_text.unwrap_or_default()}
                                                            </p>
                                                            {
                                                                if let Some(reply) = review.host_reply_text {
                                                                    view! {
                                                                        <div class="mt-2 ml-4 p-4 bg-base-200/50 rounded-lg border-l-4 border-primary">
                                                                            <div class="flex items-center gap-2 mb-2">
                                                                                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 text-primary" viewBox="0 0 20 20" fill="currentColor">
                                                                                    <path fill-rule="evenodd" d="M18 10c0 3.866-3.582 7-8 7a8.841 8.841 0 01-4.083-.98L2 17l1.338-3.123C2.493 12.767 2 11.434 2 10c0-3.866 3.582-7 8-7s8 3.134 8 7zM7 9H5v2h2V9zm8 0h-2v2h2V9zM9 9h2v2H9V9z" clip-rule="evenodd" />
                                                                                </svg>
                                                                                <span class="font-bold text-base-content text-sm">"Response from Host"</span>
                                                                            </div>
                                                                            <p class="text-base-content/80 whitespace-pre-line text-sm leading-relaxed">
                                                                                {reply}
                                                                            </p>
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
                                }
                                Err(_) => view! { <div>"Failed to load reviews."</div> }.into_any(),
                            })}
                        </Suspense>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }
        </div>
    }
}

#[component]
fn RatingBar(label: &'static str, value: f64) -> impl IntoView {
    view! {
        <div class="flex items-center justify-between w-full">
            <span class="text-base-content/80 w-1/2">{label}</span>
            <div class="flex items-center gap-3 w-1/2 justify-end">
                <div class="w-full bg-base-300 rounded-full h-1">
                    <div class="bg-base-content h-1 rounded-full" style=format!("width: {}%", (value / 5.0) * 100.0)></div>
                </div>
                <span class="text-sm font-semibold w-6 text-right">{format!("{:.1}", value)}</span>
            </div>
        </div>
    }
}
