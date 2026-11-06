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
    FxRateUnavailable {
        /// Currency to convert from.
        base: String,
        /// Currency to convert to.
        target: String,
    },
    /// Requested calendar dates conflict with an existing reservation.
    #[error("The requested dates are no longer available")]
    DateUnavailable,
    /// The listing is inactive, so it can't be quoted or held.
    #[error("This listing is not available for booking")]
    ListingUnavailable,
    /// Target booking cannot be modified in its current state or dates.
    #[error("Booking is not in a modifiable state: {0}")]
    BookingNotModifiable(String),
    /// A pending reservation change request already exists for this booking.
    #[error("A reservation change request is already pending for this booking")]
    ChangeRequestAlreadyPending,
    /// The pending hold lapsed before the booking was confirmed.
    #[error("The reservation hold has expired")]
    HoldExpired,
    /// Admin addressed a currency code that does not exist (404).
    #[error("Currency not found: {0}")]
    CurrencyNotFound(String),
    /// A write referenced a currency code that does not exist (422).
    #[error("Unknown currency: {0}")]
    CurrencyUnknown(String),
    /// A write referenced a deactivated currency.
    #[error("Currency is inactive: {0}")]
    CurrencyInactive(String),
    /// Deactivation or pair retirement is blocked by live listings.
    #[error("Currency is in use by {} listing(s)", .0.len())]
    CurrencyInUse(Vec<common::currency::ListingRef>),
    /// The currency code already exists.
    #[error("Currency already exists: {0}")]
    CurrencyExists(String),
    /// A rate's effective time is before now minus the grace period.
    #[error("Effective time is in the past")]
    EffectiveAtInPast,
    /// A rate's effective time is beyond the scheduling horizon.
    #[error("Effective time is too far in the future")]
    EffectiveAtTooFar,
    /// A rate version can no longer be deleted because it has taken effect.
    #[error("Rate version has already taken effect")]
    RateAlreadyEffective,
    /// A rate version with the same pair and effective time exists.
    #[error("A rate version for this pair and effective time already exists")]
    RateVersionExists,
    /// The new rate moves more than the guard threshold and was not confirmed.
    #[error("Rate change of {deviation_pct:?}% needs confirmation")]
    RateChangeConfirmationRequired {
        /// The rate the new one is compared with.
        baseline: rust_decimal::Decimal,
        /// The rate submitted.
        proposed: rust_decimal::Decimal,
        /// Absolute deviation in percent, for display.
        deviation_pct: Option<rust_decimal::Decimal>,
    },
    /// A promo code can't be used for this booking.
    #[error("Promo code refused: {0}")]
    PromoRefused(common::promo::PromoRefusal),
    /// A code with this string already exists, in any case.
    #[error("A promo code with this value already exists")]
    PromoCodeExists,
    /// The promo code has already been deactivated.
    #[error("This promo code has already been deactivated")]
    PromoAlreadyDeactivated,
    /// Stay dates, a guest count or search text were refused.
    #[error("Stay input refused: {0}")]
    StayInput(common::availability::StayInputError),
    /// An enquiry or request action was refused (#105).
    #[error("Request refused: {0}")]
    RequestRefused(common::booking_request::RequestRefusal),
    /// A lock wait timed out or a deadlock was broken; safe to retry.
    #[error("Database lock contention during {0}")]
    LockContention(String),
}

/// SQLSTATE: restrict violation (`ON DELETE RESTRICT`).
pub const SQLSTATE_RESTRICT_VIOLATION: &str = "23001";
/// SQLSTATE: foreign key violation.
pub const SQLSTATE_FOREIGN_KEY_VIOLATION: &str = "23503";
/// SQLSTATE: unique violation.
pub const SQLSTATE_UNIQUE_VIOLATION: &str = "23505";
/// SQLSTATE: deadlock detected.
pub const SQLSTATE_DEADLOCK_DETECTED: &str = "40P01";
/// SQLSTATE: lock not available (lock_timeout).
pub const SQLSTATE_LOCK_NOT_AVAILABLE: &str = "55P03";

/// The SQLSTATE of a database error, if it is one.
pub fn sqlstate(e: &sqlx::Error) -> Option<String> {
    e.as_database_error()
        .and_then(|db| db.code())
        .map(|code| code.into_owned())
}

impl DbError {
    /// Maps lock timeouts and deadlocks to retryable [`DbError::LockContention`]
    /// (naming `operation`); every other error is wrapped unchanged.
    pub fn from_sqlx_contended(e: sqlx::Error, operation: &str) -> Self {
        match sqlstate(&e).as_deref() {
            Some(SQLSTATE_DEADLOCK_DETECTED | SQLSTATE_LOCK_NOT_AVAILABLE) => {
                tracing::warn!(operation, error = %e, "database lock contention");
                DbError::LockContention(operation.to_string())
            }
            _ => DbError::Sqlx(e),
        }
    }
}

// A custom `Result` type that defaults to using our `DbError`.
/// A result with a [`DbError`].
pub type Result<T> = std::result::Result<T, DbError>;
