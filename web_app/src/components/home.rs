use crate::components::hero::Hero;
use crate::models::ListingResponse;
use leptos::either::Either;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;

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
            <Title text="Home" />
            <Hero />
            <h1>"Welcome to Leptos!"</h1>
            <button class="btn btn-primary" on:click=on_click>"Click Me: " {move || count.get()}</button>

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
    use crate::api_client::get_client;
    use uuid::Uuid;

    let listing_api_url =
        std::env::var("LISTING_API_URL").unwrap_or("http://localhost:8082".to_string());
    let listing_api_url = listing_api_url.trim_end_matches('/').to_string();

    // Server-side logging
    tracing::info!("LISTING_API_URL: {}", listing_api_url);

    let url = format!("{}/api/v1/listings?page=1&per_page=10", listing_api_url);
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
