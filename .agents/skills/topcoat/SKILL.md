---
name: topcoat
description: Build Topcoat SSR web apps, view! templates, signals, shards, and HTMX integration in Rust.
---

# Topcoat Framework Guide (Topcoat 0.8)

Build high-performance, server-rendered web applications using Tokio's Topcoat framework in Rust.

## Core Rules & Invariants

1. **Signals & Two-Way Reactivity (Topcoat 0.8)**:
   - Signals are created using `let name = signal(cx, || initial_value);` in ordinary Rust code above `view!`. (The legacy `signal name = value;` syntax inside views is removed).
   - **Tracked Reads on Server**: Calling `name.get()` or `name.read()` in Rust outside `$(...)` marks the `#[page]` or `#[shard]` as reactive to browser updates (`@input`, `@change`, `@click`).
   - **Untracked Reads**: Use `name.get_untracked()` or `name.read_untracked()` if the current render does not need to re-run on subsequent client updates.
   - Client expressions inside `$(...)` run exclusively in the browser.

2. **DOM Morphing & Pinning (`id`)**:
   - Re-renders morph into the DOM rather than destroying/replacing innerHTML, preserving active input focus, text selection, and scroll position.
   - Match and pin list items or dynamic blocks with explicit `id` attributes (e.g. `<div id=(format!("card-{}", item.id))>`).

3. **Shards (`#[shard]`)**:
   - Define sub-page reactive boundaries with `#[topcoat::runtime::shard]`.
   - Signals created within shards maintain stable identity across argument changes and re-renders.

4. **Router Builder (`.runtime()`)**:
   - Always mount `.runtime()` on the router: `Router::builder().runtime().discover()...build()`.
   - In layouts, include `topcoat::runtime::script()` inside the `<head>`.

5. **Form & Query Parameters**:
   - Blank browser inputs (`?status=&page=`) automatically deserialize as `None` for `Option<T>` fields.

6. **Path Parameters (`path_param!`)**:
   - Declare the parameter struct with `path_param!(name)` and extract it from context: `let slug: &str = path_param::<Slug>(cx);`

7. **Template Expressions (`view!`)**:
   - Parenthesize all raw Rust expressions or script string interpolations: `<script>(r#"..."#)</script>` or `<h1>"User: " (user_id)</h1>`.

---

## Workflow Checklist

1. **Routing & Shards**: Define `#[page("/path")]`, `#[layout]`, or `#[shard]`.
2. **Context & State**: Access request headers, query params (`query_param!`), and DB pools via `&Cx`.
3. **Reactivity**: Create signals via `signal(cx, || val)`. Bind inputs with `:value=$(sig.get()) @input=$(|e: Event| sig.set(e.target.value))`.
4. **Layout Composition**: Wrap views in `#[layout]` handlers composing child content via `(slot)`.

---

## Detailed Recipes & API References
* For detailed code examples, query param handling, shared state, signals, and shards, see **[REFERENCE.md](REFERENCE.md)**.

