# Topcoat Framework Reference & Code Recipes

## 1. Dynamic & Typed Path Parameters

### Basic Path Parameter Extraction
```rust
use topcoat::{Result, context::Cx, router::{page, path_param}, view::view};

path_param!(slug);

#[page("/listings/{slug}")]
pub async fn listing_detail(cx: &Cx) -> Result {
    let slug: &str = path_param::<Slug>(cx);
    view! {
        <h1>"Viewing listing: " (slug)</h1>
    }
}
```

### Typed Path Parameter with Error Mapping
```rust
use topcoat::{Result, context::Cx, router::{page, path_param}, view::view};

path_param!(post_id: u64, error = not_found);

#[page("/posts/{post_id}")]
pub async fn post_page(cx: &Cx) -> Result {
    let post_id: &u64 = path_param::<PostId>(cx)?;
    view! { "Post ID: " (post_id) }
}
```

---

## 2. Query Parameters (`query_param!`)

```rust
use topcoat::{Result, context::Cx, router::{page, query_param}, view::view};

query_param!(tab: String);
query_param!(page_num: Option<u32>);

#[page("/dashboard")]
pub async fn dashboard_page(cx: &Cx) -> Result {
    let current_tab: Option<&String> = query_param::<Tab>(cx);
    let page: Option<&u32> = query_param::<PageNum>(cx);
    view! {
        <div>"Tab: " (current_tab.map(|s| s.as_str()).unwrap_or("overview"))</div>
    }
}
```

---

## 3. Request Context (`Cx`) & Application State

```rust
use topcoat::{Result, context::Cx, router::page, view::view};

#[page("/api/data")]
pub async fn get_data(cx: &Cx) -> Result {
    // Access headers, cookies, request properties, and extension singletons
    let user_agent = cx.headers().get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or("unknown");
    view! {
        <p>"User Agent: " (user_agent)</p>
    }
}
```

---

## 4. HTMX Integration & Partial Fragments

```rust
use topcoat::{Result, context::Cx, router::page, view::view};

#[page("/htmx/quote")]
pub async fn quote_fragment(cx: &Cx) -> Result {
    // Returns only the partial HTML for HTMX swapping
    view! {
        <div id="quote-result" class="fade-in">
            <span class="badge badge-success">"Updated Live Quote"</span>
        </div>
    }
}
```

---

## 5. Layout Nesting & Slots

```rust
use topcoat::{Result, context::Cx, router::layout, view::view};

#[layout]
pub async fn root_layout(cx: &Cx, slot: Result) -> Result {
    view! {
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <title>"Our Places"</title>
            </head>
            <body>
                <main class="container mx-auto">
                    (slot)
                </main>
            </body>
        </html>
    }
}
```
