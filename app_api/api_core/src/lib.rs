//! Middleware, errors, auth and startup shared by the API services.
#![deny(missing_docs)]

/// Health, info, logger and metrics endpoints.
pub mod actuator;
/// Middleware shared by the APIs.
pub mod api_common;
/// Token checks and request authentication.
pub mod auth;
/// Publishes queued emails to Pub/Sub.
pub mod email_publisher;
/// The error type handlers return, and how it becomes a response.
pub mod error;
/// Maps database models to API responses.
pub mod models;
/// Pagination query parameters.
pub mod pagination;
/// Content-negotiated responses.
pub mod response;
pub mod settings;
pub mod startup;
/// Reading the environment.
pub mod sys;
/// Tracing setup and the request root span.
pub mod tracing_utils;
