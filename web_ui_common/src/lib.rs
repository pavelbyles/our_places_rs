//! Topcoat UI components, themes, client script and shared shells for the
//! redesigned guest and admin portals.
#![deny(missing_docs)]

/// Reusable view components.
pub mod components;

/// Light / dim / dark theme selection, rendered on the server.
pub mod theme;

/// Content-Security-Policy and hardening headers for both portals.
pub mod security_headers;

/// Hashed client assets: `ui.js` and vendored htmx.
pub mod assets;

/// Self-hosted Newsreader and Inter.
pub mod fonts;

/// Guest portal page shell.
pub mod shell_guest;

/// Admin portal page shell.
pub mod shell_admin;

/// Guard failure → 303 / HX-Redirect / 403 / 503.
pub mod guard;
