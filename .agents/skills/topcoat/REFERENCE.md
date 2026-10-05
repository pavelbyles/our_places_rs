# Topcoat Framework Reference & Code Recipes (Topcoat 0.8)

## 1. Two-Way Reactive Signals (`signal`)

```rust
use topcoat::{
    Result,
    context::Cx,
    router::page,
    runtime::{Event, signal},
    view::{View, view},
};

#[page("/search")]
pub async fn search_page(cx: &Cx) -> Result<impl View> {
    // 1. Initialize signal with &Cx and closure
    let query = signal(cx, String::new);

    // 2. Server-side tracked read: page automatically re-runs on client mutations
    let current_query = query.get();
    let results = load_results(cx, &current_query).await?;

    Ok(view! {
        // 3. Client bindings: :value reads client signal, @input updates it
        <input
            type="text"
            :value=$(query.get())
            @input=$(|e: Event| query.set(e.target.value))
            placeholder="Type to filter..."
        />

        // 4. Element pinning with id for in-place DOM morphing
        <div id="results-container">
            for item in results {
                <div id=(format!("item-{}", item.id)) class="card">
                    (item.title)
                </div>
            }
        </div>
    })
}
```

---

## 2. Reactive Shards (`#[shard]`)

```rust
use topcoat::{
    Result,
    context::Cx,
    runtime::{shard, signal},
    view::{View, view},
};

#[shard]
pub async fn paginated_ledger(cx: &Cx) -> Result<impl View> {
    let page = signal(cx, || 1.0);
    // Tracked read inside shard limits re-renders to only this shard
    let items = load_page(cx, page.get() as u32).await?;

    Ok(view! {
        <div id="ledger-shard">
            for item in items {
                <div id=(format!("row-{}", item.id))>(item.name)</div>
            }
            <button @click=$(|_e| page.decrement())>"Prev"</button>
            <button @click=$(|_e| page.increment())>"Next"</button>
        </div>
    })
}
```

---

## 3. Dynamic & Typed Path Parameters

```rust
use topcoat::{Result, context::Cx, router::{page, path_param}, view::view};

path_param!(slug);

#[page("/listings/{slug}")]
pub async fn listing_detail(cx: &Cx) -> Result<impl View> {
    let slug: &str = path_param::<Slug>(cx);
    Ok(view! {
        <h1>"Viewing listing: " (slug)</h1>
    })
}
```

---

## 4. Query Parameters with Empty Value Parsing

In Topcoat 0.8, empty query params (`?page=&status=`) automatically parse as `None` for `Option<T>`:

```rust
use topcoat::{Result, context::Cx, router::{page, parse_query_params}, view::view};

#[derive(serde::Deserialize, Default)]
pub struct FilterParams {
    pub search: Option<String>,
    pub page: Option<u32>,
}

#[page("/dashboard")]
pub async fn dashboard_page(cx: &Cx) -> Result<impl View> {
    let filters = parse_query_params::<FilterParams>(cx).unwrap_or_default();
    Ok(view! {
        <div>"Page: " (filters.page.unwrap_or(1))</div>
    })
}
```

---

## 5. Router Runtime Activation & Layout

```rust
use topcoat::{
    router::{Router, RouterBuilderDiscoverExt},
    runtime::RouterBuilderRuntimeExt,
};

// In main.rs:
let router = Router::builder()
    .runtime() // <-- Mandatory in Topcoat 0.8
    .discover()
    .build();

// In layout.rs:
view! {
    <html>
        <head>
            topcoat::runtime::script() // <-- Injects runtime scripts
        </head>
        <body>(slot)</body>
    </html>
}
```

