use crate::components::hero::Hero;
use crate::models::ListingResponse;
use leptos::either::Either;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use num_format::{Locale, ToFormattedString};
use rust_decimal::prelude::ToPrimitive;
use web_app_common::components::villa_card::VillaCard;

#[component]
#[allow(non_snake_case)]
pub fn HomePage() -> impl IntoView {
    let count = RwSignal::new(0);
    let on_click = move |_| {
        spawn_local(async move {
            let new_count = update_count(count.get_untracked()).await.unwrap_or(0);
            count.set(new_count);
        });
    };

    let listings = Resource::new(|| (), |_| async move { fetch_listings().await });

    view! {
        <>
            <Title text="Home" />
            <Hero />

            <div class="py-16 max-w-6xl mx-auto px-4 flex flex-col gap-12">
                <div class="text-center max-w-2xl mx-auto flex flex-col gap-3">
                    <span class="text-primary font-semibold tracking-wider uppercase text-sm">"Explore Stays"</span>
                    <h2 class="text-4xl font-extrabold tracking-tight">"Featured Places to Stay"</h2>
                    <p class="text-base-content/60">"Handpicked properties with premium amenities for a perfect getaway."</p>
                </div>

                <Suspense fallback=move || view! { <div class="flex justify-center py-12"><span class="loading loading-spinner loading-lg text-primary"></span></div> }>
                    {move || {
                        listings.get().map(|result| {
                            match result {
                                Ok(items) => {
                                    if items.is_empty() {
                                        Either::Left(view! {
                                            <div class="text-center py-12 opacity-60 text-lg">"No featured listings found"</div>
                                        }.into_any())
                                    } else {
                                        Either::Left(view! {
                                            <div class="grid grid-cols-1 md:grid-cols-2 gap-8">
                                                {items.into_iter().map(|item| {
                                                    let price_str = item.price_per_night
                                                        .map(|p| p.to_i64().unwrap().to_formatted_string(&Locale::en))
                                                        .unwrap_or_else(|| "0.00".to_string());
                                                    let img_url = item.primary_image_url.clone().unwrap_or_else(|| "https://images.unsplash.com/photo-1499793983690-e29da59ef1c2?ixlib=rb-4.0.3&auto=format&fit=crop&w=800&q=80".to_string());
                                                    view! {
                                                        <VillaCard
                                                            title=item.name.clone()
                                                            image_url=img_url
                                                            price=price_str
                                                            max_guests=item.max_guests
                                                            bedrooms=item.bedrooms
                                                            full_bathrooms=item.full_bathrooms
                                                            country=item.country.clone()
                                                            city=item.city.clone()
                                                            id=item.slug.clone()
                                                            currency=item.base_currency.clone()
                                                            rating=item.overall_rating
                                                        />
                                                    }
                                                }).collect_view()}
                                            </div>
                                        }.into_any())
                                    }
                                }
                                Err(e) => Either::Right(view! {
                                    <div class="alert alert-error shadow-md max-w-lg mx-auto">
                                        <span>"Error loading listings: " {e.to_string()}</span>
                                    </div>
                                })
                            }
                        })
                    }}
                </Suspense>

                // Counter section styled beautifully
                <div class="card bg-base-200 border border-base-300 p-8 text-center max-w-md mx-auto mt-8 flex flex-col items-center gap-4">
                    <h3 class="text-xl font-bold">"Interactive Demo"</h3>
                    <p class="text-sm text-base-content/70">"Test reactivity with a server function call that persists/updates count:"</p>
                    <button class="btn btn-primary" on:click=on_click>
                        "Click Counter: " {move || count.get()}
                    </button>
                </div>
            </div>
        </>
    }
}

#[server]
pub async fn update_count(count: i32) -> Result<i32, ServerFnError> {
    Ok(count + 1)
}

#[server]
#[tracing::instrument]
pub async fn fetch_listings() -> Result<Vec<ListingResponse>, ServerFnError> {
    use uuid::Uuid;
    use web_app_common::api_client::get_client;

    let listing_api_url =
        std::env::var("LISTING_API_URL").unwrap_or("http://localhost:8082".to_string());
    let listing_api_url = listing_api_url.trim_end_matches('/').to_string();

    // Server-side logging
    tracing::info!("LISTING_API_URL: {}", listing_api_url);

    let mut url = format!("{}/api/v1/listings?page=1&per_page=10", listing_api_url);

    #[cfg(feature = "ssr")]
    if let Ok(session) = leptos_actix::extract::<actix_session::Session>().await {
        if let Ok(Some(currency)) = session.get::<String>("user_default_currency") {
            url.push_str(&format!("&currency={}", currency));
        }
    }
    let request_id = Uuid::new_v4();

    tracing::info!(
        "Fetching listings from {} with trace-id: {}",
        url,
        request_id
    );

    // Log Request Details
    tracing::info!("Request URL: {}", url);

    let audience = listing_api_url.clone();
    let client = get_client();

    let res = client
        .get_request(&url, &audience)
        .await
        .map_err(|e| ServerFnError::new(format!("Auth error: {}", e)))?
        .header("trace-id", request_id.to_string())
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("Request failed: {}", e)))?;

    // Log Response Details
    let status = res.status();

    let text = res
        .text()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to read body: {}", e)))?;

    if !status.is_success() {
        tracing::error!("API Error {}: {}", status, text);
        return Err(ServerFnError::new(format!(
            "API Error {}: {}",
            status, text
        )));
    }

    serde_json::from_str::<Vec<ListingResponse>>(&text)
        .map_err(|e| ServerFnError::new(format!("Failed to parse JSON: {} | Body: {}", e, text)))
}
