use std::fmt;

/// Application error type for Topcoat SSR handlers implementing std::error::Error.
/// This bridges string/anyhow errors to Topcoat 0.10.0's StdError requirement.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct AppError(pub String);

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self(err.to_string())
    }
}

pub fn app_error(msg: impl fmt::Display) -> AppError {
    AppError(msg.to_string())
}
