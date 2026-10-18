use thiserror::Error;

/// Database error variants across domain queries and transactions.
#[derive(Debug, Error)]
pub enum DbError {
    /// Underlying SQLx database error.
    #[error("Database error")]
    Sqlx(#[from] sqlx::Error),
    /// Domain validation failure.
    #[error("Validation error: {0}")]
    ValidationError(String),
    /// Foreign exchange rate unavailable for requested currency pair.
    #[error("Foreign exchange rate unavailable for currency pair: {base} -> {target}")]
    FxRateUnavailable { base: String, target: String },
    /// Requested calendar dates conflict with an existing reservation.
    #[error("The requested dates are no longer available")]
    DateUnavailable,
    /// Target booking cannot be modified in its current state or dates.
    #[error("Booking is not in a modifiable state: {0}")]
    BookingNotModifiable(String),
    /// A pending reservation change request already exists for this booking.
    #[error("A reservation change request is already pending for this booking")]
    ChangeRequestAlreadyPending,
}

// A custom `Result` type that defaults to using our `DbError`.
pub type Result<T> = std::result::Result<T, DbError>;
