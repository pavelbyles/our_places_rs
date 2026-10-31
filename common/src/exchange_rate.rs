//! Currency exchange rates as stored in `currency_exchange_rates`.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The latest rate for one currency pair: 1 `base_currency` buys `rate`
/// `target_currency`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ExchangeRate {
    /// ISO 4217 code being converted from.
    pub base_currency: String,
    /// ISO 4217 code being converted to.
    pub target_currency: String,
    /// Units of `target_currency` per unit of `base_currency`.
    #[schema(value_type = String)]
    pub rate: Decimal,
    /// When this rate took effect.
    pub effective_at: DateTime<Utc>,
}

/// The latest rate for every stored currency pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ExchangeRatesResponse {
    /// Sorted by base then target currency.
    pub rates: Vec<ExchangeRate>,
}
