---
name: topcoat
description: Build web applications using Tokio's Topcoat Rust framework. Use when writing, debugging, or reviewing Topcoat SSR pages, view! templates, routes, dynamic path parameters (path_param!), query parameters (query_param!), layouts, HTMX integration, request context (Cx), app state (app_context), and module routers.
---

# Topcoat Framework Guide

## Quick Start & Mandatory Rules

### 1. Dynamic Path Parameters (`path_param!`)
> **CRITICAL RULE**: Topcoat does **not** inject path parameters as function arguments. You must declare the parameter type with `path_param!(name)` and read it inside the handler via `path_param::<Type>(cx)`.

```rust
use topcoat::{
    Result,
    context::Cx,
    router::{page, path_param},
    view::view,
};

// 1. Declare parameter (generates typed struct Slug)
path_param!(slug);

// 2. Define route
#[page("/listings/{slug}")]
pub async fn listing_detail(cx: &Cx) -> Result {
    // 3. Extract decoded parameter from request context
    let slug: &str = path_param::<Slug>(cx);
    
    view! {
        <h1>"Viewing listing: " (slug)</h1>
    }
}
```

### 2. Typed Path Parameters with Error Handling
```rust
path_param!(post_id: u64, error = not_found);

#[page("/posts/{post_id}")]
pub async fn post_page(cx: &Cx) -> Result {
    let post_id: &u64 = path_param::<PostId>(cx)?;
    view! { "Post ID: " (post_id) }
}
```

### 3. Query Parameters (`query_param!`)
```rust
use topcoat::router::query_param;

query_param!(tab: String);

#[page("/dashboard")]
pub async fn dashboard_page(cx: &Cx) -> Result {
    let current_tab: Option<&String> = query_param::<Tab>(cx);
    view! { ... }
}
```

### 4. Request Context (`Cx`) & Application State (`app_context`)
- `Cx` provides request metadata, headers, query parameters, path segments, and extension data.
- Global application context / shared dependencies:
```rust
use topcoat::context::Cx;

#[page("/api/data")]
pub async fn get_data(cx: &Cx) -> Result {
    // Extract shared state / database pool from context
    // Access headers, cookies, and request properties via cx
    view! { ... }
}
```

### 5. HTMX Integration (`htmx`)
Topcoat includes first-class HTMX support for fragments, triggers, swaps, and target responses:
```rust
use topcoat::{Result, context::Cx, router::page, view::view};

#[page("/htmx/quote")]
pub async fn quote_fragment(cx: &Cx) -> Result {
    // Returns only the partial HTML for HTMX swapping
    view! {
        <div id="quote-result" class="fade-in">
            "Updated Live Quote"
        </div>
    }
}
```

### 6. Layouts & View Macro (`view!`)
- **Raw script/string interpolation**: Embedded script strings or raw Rust expressions in `view!` must be parenthesized: `<script>(r#"..."#)</script>`.
- **Layout definitions**:
```rust
use topcoat::{Result, context::Cx, router::layout, view::view};

#[layout]
pub async fn root_layout(cx: &Cx, slot: Result) -> Result {
    view! {
        <html lang="en">
            <body>
                <main class="container">
                    (slot)
                </main>
            </body>
        </html>
    }
}
```

---

## Detailed References
Full specifications from the official Topcoat documentation are bundled in `references/`:

### Core Runtime & Context
- [Request Context (`context.md`)](references/context.md): Request context (`Cx`), lifetimes, metadata, and per-request memoization.
- [App Context (`app_context.md`)](references/app_context.md): Application-level state, shared singletons, database pools, and services.
- [Runtime (`runtime.md`)](references/runtime.md): Async executor integration, task spawning, and runtime architecture.
- [Getting Started (`getting_started.md`)](references/getting_started.md): Project initialization, configuration, and build pipeline.

### Routing & Parameters
- [Router Architecture (`router.md`)](references/router.md): Full router specification, matching precedence, and pipeline execution.
- [Path Parameters (`path_param.md`)](references/path_param.md): `path_param!` macro, custom parsers, error mappings, and catch-alls.
- [Query Parameters (`query_params.md`)](references/query_params.md): `query_param!` macro, URL decoding, and optional types.
- [Pages (`page.md`)](references/page.md): `#[page]` attribute macro, route declarations, and endpoints.
- [Module Router (`module_router.md`)](references/module_router.md): Filesystem-based automatic route discovery and module hierarchies.
- [Layouts (`layout.md`)](references/layout.md): Nested layout hierarchies, slots, and layout composition.
- [Routing Errors (`error.md`)](references/error.md): Error responses, `RouterErrorExt`, and status codes (404, 401, 403, 500).
- [Layers & Middleware (`layer.md`)](references/layer.md): Request interceptors, layers, and tower middleware.

### Frontend, HTMX & UI
- [HTMX (`htmx.md`)](references/htmx.md): HTMX partial rendering, swap targets, headers, and out-of-band updates.
- [UI Components (`ui.md`)](references/ui.md): Component abstractions, component props, and markup utilities.
- [View Templates (`view.md`)](references/view.md): `view!` macro syntax, control flow, loops, and escaping.
- [Components (`component.md`)](references/component.md): Functional SSR components and composability.
- [Tailwind CSS (`tailwind.md`)](references/tailwind.md): Tailwind CSS integration, asset bundling, and theme support.
- [Sessions (`session.md`)](references/session.md) & [Cookies (`cookie.md`)](references/cookie.md): Cookie encryption, session management, and auth tokens.
