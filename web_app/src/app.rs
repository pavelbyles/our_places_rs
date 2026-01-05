use leptos::prelude::*;
use leptos::either::Either;
use leptos::task::spawn_local;
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment, WildcardSegment,
};
use crate::models::ListingResponse;




#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/web_app.css"/>

        // sets the document title
        <Title text="Welcome to Leptos"/>

        // content for this welcome page
        <Router>
            <main>
                <Routes fallback=move || "Not found.">
                    <Route path=StaticSegment("") view=HomePage/>
                    <Route path=WildcardSegment("any") view=NotFound/>
                </Routes>
            </main>
        </Router>
    }
}

/// Renders the home page of your application.
#[component]
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
    let listings = Resource::new(
        || (),
        |_| async move { fetch_listings().await }
    );

    view! {
        <h1>"Welcome to Leptos!"</h1>
        <button on:click=on_click>"Click Me: " {count}</button>
        
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
    }
}

#[server]
pub async fn update_count(count: i32) -> Result<i32, ServerFnError> {
    Ok(count + 1)
}

/// 404 - Not Found
#[component]
pub fn NotFound() -> impl IntoView {
    // set an HTTP status code 404
    // this is feature gated because it can only be done during
    // initial server-side rendering
    // if you navigate to the 404 page subsequently, the status
    // code will not be set because there is not a new HTTP request
    // to the server
    #[cfg(feature = "ssr")]
    {
        // this can be done inline because it's synchronous
        // if it were async, we'd use a server function
        let resp = expect_context::<leptos_actix::ResponseOptions>();
        resp.set_status(actix_web::http::StatusCode::NOT_FOUND);
    }

    view! {
        <h1>"Not Found"</h1>
    }
}


#[server]
pub async fn fetch_listings() -> Result<Vec<ListingResponse>, ServerFnError> {
    use reqwest;
    use uuid::Uuid;

    let listing_api_url = option_env!("LISTING_API_URL").unwrap_or("http://localhost:8082");
    
    // Server-side logging
    println!("LISTING_API_URL: {}", listing_api_url);
    
    let url = format!("{}/api/v1/listings/?page=1&per_page=10", listing_api_url);
    let request_id = Uuid::new_v4();
    
    println!("Fetching listings with trace-id: {}", request_id);
    
    reqwest::Client::new()
        .get(&url)
        .header("trace-id", request_id.to_string())
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .json::<Vec<ListingResponse>>()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}