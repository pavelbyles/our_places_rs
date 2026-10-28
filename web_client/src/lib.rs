//! UI-free web client shared by the Topcoat portals: backend API client,
//! application errors, and session/auth extraction.
#![deny(missing_docs)]

/// Backend API wrappers and request helpers.
pub mod api_client;
/// HTTP client for the backend services, registered in Topcoat's app context.
pub mod client;
/// Application error type for Topcoat SSR handlers.
pub mod error;

pub use client::{TopcoatApiClient, get_api_client};
pub use error::{AppError, app_error};
