# Topcoat Framework Reference & Code Recipes (Topcoat 0.12)

## 1. Component (`cx` first)

```rust
use topcoat::{
    Result,
    context::Cx,
    view::{Attributes, Child, View, class, component, view},
};

/// A labelled stat.
#[component]
pub async fn stat_tile(
    cx: &Cx,
    /// What the number measures.
    label: String,
    /// The formatted value.
    value: String,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
    /// Optional supporting content.
    #[default]
    child: Child<'_>,
) -> Result<impl View> {
    let class = class!("stat", attrs.remove("class"));
    Ok(view! {
        <div class=(class) (attrs)>
            <div class="stat-title">(label)</div>
            <div class="stat-value">(value)</div>
            (child)
        </div>
    })
}

// Call site: comma between the last prop and the first child.
// view! { stat_tile(label: "Nights".into(), value: n.to_string(), "this month") }
```

---

## 2. Page with a path parameter and guard

```rust
use topcoat::{Result, context::Cx, router::{page, path_param}, view::{View, view}};
use uuid::Uuid;

path_param!(id); // generates `fn id(cx: &Cx) -> &str`

#[page("/admin/listings/{id}")]
pub async fn listing(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let listing_id = Uuid::parse_str(id(cx)).map_err(|_| not_found())?;
    // ...
    Ok(view! { <h1>"Listing " (listing_id.to_string())</h1> })
}
```

The typed form parses once per request and maps failures for you:

```rust
path_param!(pub id: Uuid else not_found);
let listing_id = id(cx)?;
```

---

## 3. Context values

```rust
use topcoat::context::{Cx, context, try_context, iter_context};

// Router-wide (main.rs): .with(TopcoatApiClient::from_env())
let client = try_context::<TopcoatApiClient>(cx);

// Per-route value read by an outer layout:
struct Crumb(&'static str);

#[page("/admin/payouts", with(Crumb("Payouts")))]
async fn payouts(cx: &Cx) -> Result<impl View> { /* ... */ }

// In the layout: visit every Crumb, closest first.
for Crumb(label) in iter_context::<Crumb>(cx) { /* ... */ }
```

Values set with `.with(..)` are built once with the router and can't read the request. For request-dependent values, store a `BoxCxFn`, or call a `#[memoize]` function.

---

## 4. Router wiring (portal `main.rs`)

```rust
use topcoat::asset::{AssetBundle, RouterBuilderAssetExt};
use topcoat::cookie::CookieLayer;
use topcoat::router::{Router, RouterBuilderDiscoverExt};
use topcoat::session::{SessionTokenStore, cookie::CookieTokenStore};

let router = Router::builder()
    .discover()
    .layer(CookieLayer::new())
    .with(SessionTokenStore::new(CookieTokenStore::new().name("op_admin_session")))
    .with(TopcoatApiClient::from_env())
    .with(service_jwt)
    .assets(AssetBundle::load()?)
    .layer(SecurityHeaders::admin().with_hsts(hsts))
    .layer(web_ui_common::request_tracing::RequestTracing)
    .build();
```

Layer order: pathless layers (`SecurityHeaders`, `RequestTracing`) wrap path layers (`CookieLayer` sits at `/`). Among layers at the same path, the last one registered runs outermost. Never add `RuntimeLayer` (CSP).

Per-route settings use `with(...)`:

```rust
#[route(POST "/upload", with(BodyLimit::max(32 * 1024 * 1024)))]
#[route(GET "/export.csv", with(Compression::disable()))]
```

---

## 5. Cookies

```rust
use topcoat::cookie::{Cookie, SameSite, cookies, time::Duration};

cookies(cx).add(
    Cookie::build(("op_currency", code.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::days(365)),
);
```

`cookies(cx)` panics if the router has no `CookieLayer`. In code that may run against a bare `Cx`, use `try_cookies(cx)`.

---

## 6. Query parameters

Blank inputs (`?status=&page=`) deserialize as `None` for `Option<T>`:

```rust
#[derive(serde::Deserialize, Default)]
pub struct FilterParams { pub search: Option<String>, pub page: Option<u32> }

let filters = parse_query_params::<FilterParams>(cx).unwrap_or_default();
```

---

## 7. Rendering in tests

```rust
use topcoat::context::Cx;

async fn html(v: impl topcoat::view::View, cx: &Cx) -> String { /* render to string */ }

#[tokio::test]
async fn renders_alert() {
    let cx = &Cx::default();
    let out = html(view! { coverage_alert(gaps: Vec::new()) }, cx).await;
    assert!(!out.contains("alert"));
}
```

---

## 8. Page metadata (available, not yet adopted)

```rust
use topcoat::metadata::Metadata;

#[page("/listings", with(Metadata::new().title("Villas")))]
// In the shell's <head>: topcoat::metadata::tags()
```

Canonical and Open Graph URLs need `.with(BaseUrl::new(...)?)` on the router. See `references/metadata.md`.
