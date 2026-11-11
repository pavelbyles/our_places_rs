---
name: topcoat
description: Build Topcoat 0.12 SSR pages, view! templates, components, routing, context, sessions and HTMX integration in Rust (web_ui_common / web_ui_guest / web_ui_admin).
---

# Topcoat Framework Guide (Topcoat 0.12)

The guest and admin portals are server-rendered Topcoat apps that use HTMX for partial updates. Shared components, shells and layers live in `web_ui_common`. Backend calls and session guards live in `web_client`.

## Core Rules & Invariants

1. **`cx` comes first, and it is called `cx`.**
   - Every `#[component]`, `#[page]`, `#[route]`, `#[layout]`, `#[layer]`, `#[shard]`, `#[procedure]` and `#[memoize]` function takes `cx: &Cx` as its **first** parameter, even when the body doesn't use it.
   - `view!`, `attributes!`, `live!` and `emit!` capture the `cx` variable in scope. There is no `cx =>` prefix and no implicit `__cx`.
   - To change the context of a view, shadow it: `let cx = cx.with(value);`.
   - `view!` moves `cx`. If a view must outlive the function, add `let cx = cx.clone();` before it.

2. **One context.**
   - Read values with `context::<T>(cx)`, `try_context::<T>(cx)` or `iter_context::<T>(cx)` from `topcoat::context`.
   - The old `app_context` and `request_context` functions are gone.
   - Register app-wide values on the router with `.with(value)`, e.g. `TopcoatApiClient` and `ServiceJwt` in `main.rs`.
   - Attach per-route values with `with(...)` on `#[page]`, `#[route]`, `#[layout]` or `#[layer]`. Inner values shadow outer ones.

3. **Router wiring** (`web_ui_{guest,admin}/src/main.rs`):
   ```rust
   Router::builder()
       .discover()
       .layer(CookieLayer::new())                       // replaces .cookies()
       .with(SessionTokenStore::new(CookieTokenStore::new().name("op_guest_session")))
       .with(TopcoatApiClient::from_env())
       .with(service_jwt)
       .assets(assets)
       .layer(SecurityHeaders::new() /* ... */)
       .layer(RequestTracing)
       .build()
   ```
   - **Never add `RuntimeLayer`.** It needs `'unsafe-eval'`, which the CSP forbids. That rules out signals, `$(...)` client expressions and shards in these portals. Use HTMX and `ui.js` `data-*` hooks instead.
   - The cookie store adds the `__Host-` prefix and forces `Secure; HttpOnly; SameSite=Lax; Path=/`. The session lifetime is `SessionLifetime::DEFAULT` (30 days).

4. **Path parameters.**
   - `path_param!(id);` generates an accessor function: `let raw: &str = id(cx);`.
   - Typed form: `path_param!(pub id: Uuid else not_found);` gives `id(cx)?`, parsed once per request.
   - The accessor shares its name with the parameter, so don't shadow it with a local of the same name before calling it.
   - `path_param::<Id>(cx)` and the wrapper types are removed.

5. **Macro argument style.**
   - Attribute macros take `key = value`.
   - Function-like macros take `key: value`, e.g. `t!(greeting, name: n)`, `asset!("src/x.js", checksum: "sha256:...")`, `href!(post, post_id: 42)`.
   - A comma is required between the last prop and the first child: `card(title: t, "child")`.
   - `asset!` needs a string-literal path, and a missing file is a compile error. It supports `{mod}/` and `{out_dir}/` prefixes.

6. **Sessions and cookies.**
   - Use `web_client::session::{require_guest, require_admin, require_admin_role}`. They already return `Unauthenticated` when the cookie layer is missing; never call `topcoat::cookie::cookies(cx)` without it.
   - Build cookies with `Cookie::build((name, value)).path("/")...`. The `cookie!` and `mail!` macros are removed.

7. **Template expressions.** Parenthesize Rust expressions: `<h1>"User: " (user_id)</h1>`, `class=(field)`.

8. **Tests.** Render with a bare context: `let cx = &Cx::default(); html(view! { my_component(prop: x) }, cx).await`.

9. **Formatting.** `topcoat fmt` formats every Topcoat macro. Run it in each UI crate, then `cargo fmt --all`, then `topcoat fmt` again (the two disagree on `class!` line-wrapping). Don't use `topcoat fmt --rustfmt`: it runs rustfmt without edition 2024 and fails. The CLI version must match the library (`topcoat --version` → 0.12.x).

---

## Workflow Checklist

1. **Page or route**: `#[page("/path")]` or `#[route(GET "/x.csv")]` with `cx: &Cx` first. Guard it with `staff(cx).await?`, `guest(cx).await?` or similar from `support.rs`.
2. **Data**: Make backend calls through `get_api_client(cx)`. Deduplicate per-request work with `#[memoize]`.
3. **Markup**: Use `view!` with `web_ui_common` components and DaisyUI or Tailwind classes. Wrap the page in `admin_shell(cx, ...)` / `guest_shell(cx, ...)`, which return only the slot for HTMX partial requests.
4. **Verify**: Run `cargo test -p <crate>`, then `topcoat fmt --check`, then the Playwright suite for the portal.

---

## Detailed Recipes & API References
* Code recipes: **[REFERENCE.md](REFERENCE.md)**.
* Upstream 0.12.1 guides are in `references/`. Useful starting points: `context.md`, `router.md`, `path_param.md`, `module_param.md`, `href.md`, `component.md`, `view_macro.md`, `session.md`, `cookie.md`, `metadata.md` and `fmt.md`. `functions_not_middlewares.md` explains the guard-function pattern.
