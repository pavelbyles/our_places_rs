use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Database error")]
    Sqlx(#[from] sqlx::Error),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Foreign exchange rate unavailable for currency pair: {base} -> {target}")]
    FxRateUnavailable { base: String, target: String },
    #[error("The requested dates are no longer available")]
    DateUnavailable,
    #[error("Booking is not in a modifiable state: {0}")]
    BookingNotModifiable(String),
    #[error("A reservation change request is already pending for this booking")]
    ChangeRequestAlreadyPending,
}

// A custom `Result` type that defaults to using our `DbError`.
pub type Result<T> = std::result::Result<T, DbError>;
