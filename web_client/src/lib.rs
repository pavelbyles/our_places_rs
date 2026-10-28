//! UI-free web client shared by the Topcoat portals: backend API client,
//! application errors, and session/auth extraction.
#![deny(missing_docs)]

/// Backend API wrappers and request helpers.
pub mod api_client;
/// Authenticated user model and session lookup for guest and admin portals.
pub mod auth;
/// HTTP client for the backend services, registered in Topcoat's app context.
pub mod client;
/// Application error type for Topcoat SSR handlers.
pub mod error;
/// Typed guest and admin sessions and the route guards that produce them.
pub mod session;

pub use client::{TopcoatApiClient, get_api_client};
pub use error::{AppError, app_error};
