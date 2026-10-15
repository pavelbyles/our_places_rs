use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Database error")]
    Sqlx(#[from] sqlx::Error),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Foreign exchange rate unavailable for currency pair: {base} -> {target}")]
    FxRateUnavailable { base: String, target: String },
}

// A custom `Result` type that defaults to using our `DbError`.
pub type Result<T> = std::result::Result<T, DbError>;
