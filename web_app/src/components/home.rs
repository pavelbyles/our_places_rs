use crate::components::hero::Hero;
use crate::models::ListingResponse;
use leptos::either::Either;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
#[allow(non_snake_case)]
pub fn HomePage() -> impl IntoView {
    // Creates a reactive value to update the button
    let count = RwSignal::new(0);
    let on_click = move |_| {
        spawn_local(async move {
            let new_count = update_count(count.get()).await.unwrap_or(0);
            count.set(new_count);
        });
    };

    // Creates a resource that invokes the server function to fetch listings
    let listings = Resource::new(|| (), |_| async move { fetch_listings().await });

    view! {
        <>
            <Hero />
            <h1>"Welcome to Leptos!"</h1>
            <button class="btn btn-primary" on:click=on_click>"Click Me: " {count}</button>

            <Suspense fallback=move || view! { <p>"Loading listings..."</p> }>
                {move || {
                    listings.get().map(|result| {
                        match result {
                            Ok(items) => Either::Left(view! {
                                <ul>
                                    {items.into_iter().map(|item| view! {
                                        <li>{item.name}</li>
                                    }).collect_view()}
                                </ul>
                            }),
                            Err(e) => Either::Right(view! { <p>"Error loading listings: " {e.to_string()}</p> })
                        }
                    })
                }}
            </Suspense>
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
    use reqwest;
    use uuid::Uuid;

    let listing_api_url =
        std::env::var("LISTING_API_URL").unwrap_or("http://localhost:8082".to_string());

    // Server-side logging
    tracing::info!("LISTING_API_URL: {}", listing_api_url);

    let url = format!("{}/api/v1/listings/?page=1&per_page=10", listing_api_url);
    let request_id = Uuid::new_v4();

    tracing::info!(
        "Fetching listings from {} with trace-id: {}",
        url,
        request_id
    );

    let api_key = std::env::var("API_KEY").unwrap_or_default();

    // Log Request Details
    tracing::info!("Request URL: {}", url);
    tracing::info!("Request Headers: x-api-key={}", api_key);

    let client = reqwest::Client::new();
    let res = client
        .get(&url)
        .header("x-api-key", api_key.to_string())
        .header("trace-id", request_id.to_string())
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("Request failed: {}", e)))?;

    // Log Response Details
    let status = res.status();
    tracing::info!("Response Status: {}", status);

    let text = res
        .text()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to read body: {}", e)))?;
    tracing::info!("Response Body: {}", text);

    if !status.is_success() {
        return Err(ServerFnError::new(format!(
            "API Error {}: {}",
            status, text
        )));
    }

    serde_json::from_str::<Vec<ListingResponse>>(&text)
        .map_err(|e| ServerFnError::new(format!("Failed to parse JSON: {} | Body: {}", e, text)))
}
