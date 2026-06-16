use crate::error::Result;
use rust_decimal::Decimal;
use sqlx::PgExecutor;

/// Gets the exchange rate from base_currency to target_currency.
/// If the pair does not exist, it falls back to USD as requested.
pub async fn get_exchange_rate_and_currency<'e, E>(
    executor: E,
    base_currency: &str,
    target_currency: &str,
) -> Result<(Decimal, String)>
where
    E: PgExecutor<'e> + Copy,
{
    if base_currency == target_currency {
        return Ok((Decimal::new(1, 0), target_currency.to_string()));
    }

    let rate = sqlx::query_scalar!(
        r#"
        SELECT rate FROM currency_exchange_rates
        WHERE base_currency = $1 AND target_currency = $2
        ORDER BY effective_at DESC
        LIMIT 1
        "#,
        base_currency,
        target_currency
    )
    .fetch_optional(executor)
    .await?;

    if let Some(rate) = rate {
        return Ok((rate, target_currency.to_string()));
    }

    // Fall back to USD if the requested pair is missing
    let fallback_currency = "USD";
    if base_currency == fallback_currency {
        return Ok((Decimal::new(1, 0), fallback_currency.to_string()));
    }

    // Try converting base to USD
    let rate_to_usd = sqlx::query_scalar!(
        r#"
        SELECT rate FROM currency_exchange_rates
        WHERE base_currency = $1 AND target_currency = $2
        ORDER BY effective_at DESC
        LIMIT 1
        "#,
        base_currency,
        fallback_currency
    )
    .fetch_optional(executor)
    .await?;

    if let Some(rate) = rate_to_usd {
        Ok((rate, fallback_currency.to_string()))
    } else {
        // Ultimate fallback if even base->USD is missing: return as base_currency
        Ok((Decimal::new(1, 0), base_currency.to_string()))
    }
}
