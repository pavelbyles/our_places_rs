---
name: topcoat
description: Build Topcoat SSR web apps, view! templates, HTMX integration, and module routes in Rust.
---

# Topcoat Framework Guide

Build fast, server-rendered web applications using Tokio's Topcoat framework in Rust.

## Core Rules & Invariants

1. **Path Parameters (`path_param!`)**:
   > **CRITICAL**: Topcoat does **not** inject path parameters as function arguments. Declare the parameter struct with `path_param!(name)` and extract it from context: `let slug: &str = path_param::<Slug>(cx);`

2. **Template Expressions (`view!`)**:
   - Parenthesize all raw Rust expressions or script string interpolations: `<script>(r#"..."#)</script>` or `<h1>"User: " (user_id)</h1>`.

3. **Handler Signature**:
   - All page and layout handlers accept `&Cx` (Request Context) and return `topcoat::Result`:
   ```rust
   use topcoat::{Result, context::Cx, router::page, view::view};

   #[page("/items/{slug}")]
   pub async fn item_page(cx: &Cx) -> Result {
       path_param!(slug);
       let slug: &str = path_param::<Slug>(cx);
       view! { <h1>"Item: " (slug)</h1> }
   }
   ```

## Workflow Checklist

1. **Routing**: Define `#[page("/path")]` or `#[layout]`. For dynamic segments, declare `path_param!(param_name)`.
2. **Context & State**: Access request headers, query params (`query_param!`), and DB pools via `&Cx`.
3. **HTMX Swaps**: Return targeted view fragments for partial HTMX updates.
4. **Layout Composition**: Wrap views in `#[layout]` handlers composing child content via `(slot)`.

---

## Detailed Recipes & API References
* For detailed code examples, query param handling, shared state, and HTMX swapping patterns, see **[REFERENCE.md](REFERENCE.md)**.
* For full library docs, browse the bundled guide in [references/](references/).
