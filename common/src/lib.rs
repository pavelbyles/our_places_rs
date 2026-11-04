//! Domain types, pricing and clients shared by the services and portals.
#![deny(missing_docs)]

#[cfg(not(target_arch = "wasm32"))]
pub mod app_client;
/// User permission attributes.
pub mod auth;
pub mod availability;
pub mod cancellation;
#[cfg(not(target_arch = "wasm32"))]
pub mod cloud_logging;
/// CSV export.
pub mod csv;
pub mod currency;
/// Email payloads and templates.
pub mod email;
pub mod exchange_rate;
/// Google Cloud Storage signed URLs.
#[cfg(not(target_arch = "wasm32"))]
pub mod gcs;
/// Looks up places from coordinates.
pub mod geocode;
/// Authenticated HTTP client for calls between services.
#[cfg(not(target_arch = "wasm32"))]
pub mod http_client;
/// Request and response types shared across services.
pub mod models;
/// Host payout states and ledger types.
pub mod payout;
/// Stay pricing.
pub mod pricing;
/// Promo code rules: validation, usage limits and discount amounts.
pub mod promo;
pub mod query;
/// Static reference data, such as statutory tax rates.
pub mod reference;
pub mod slug;
/// Tax document types and numbering.
pub mod tax_document;
#[cfg(not(target_arch = "wasm32"))]
pub mod telemetry;
