# Web App Development Context

## Overview
The `web_app` is a frontend application built with **Leptos** (Rust) and **Tailwind CSS + DaisyUI**.

## Key Technologies
- **Leptos**: Reactive web framework (Signals, Server Functions).
- **Leptos Router**: Client-side routing.
- **Tailwind CSS (v4)**: Utility-first styling.
- **DaisyUI (v5)**: Component library (integrated via `@plugin`).

## Directory Structure
- `src/components/`: Reusable UI components (Hero, Navbar, etc.).
- `src/components/layout.rs`: Persistent layout wrapper.
- `style/`: CSS files. `tailwind.css` is the entry point.

## Development Workflow

### Running the App
Run the development server with hot-reloading:
```bash
cargo leptos watch
```

### CSS Building
Tailwind CSS must be running in parallel to generate styles:
```bash
npm run watch:css
```
Or for a one-off build:
```bash
npm run build:css
```

### Common Patterns
- **Layout**: Use the `Layout` component in `app.rs` to wrap routes that need the Navbar/Footer.
- **Components**: Create new components in `src/components/`, export them in `mod.rs`, and use them in views.
- **Styling**: Favor utility classes in `view!` macros.
- **String Literals**: Always quote text nodes in `view!` macros (e.g., `"Hello"`) to avoid type inference issues.
