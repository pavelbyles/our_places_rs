use crate::error::Result;
use rust_decimal::Decimal;
use sqlx::PgExecutor;
use std::collections::HashMap;

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

    Ok(rate_to_usd
        .map(|rate| (rate, fallback_currency.to_string()))
        .unwrap_or_else(|| (Decimal::new(1, 0), base_currency.to_string())))
}

/// Fetches exchange rates for multiple base currencies to a target currency, caching distinct rates in a map.
pub async fn get_exchange_rates_cache<'e, E, I, S>(
    executor: E,
    base_currencies: I,
    target_currency: &str,
) -> HashMap<String, (Decimal, String)>
where
    E: PgExecutor<'e> + Copy,
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut rates = HashMap::new();
    for base in base_currencies {
        let base_ref = base.as_ref();
        #[allow(clippy::collapsible_if)]
        if !rates.contains_key(base_ref) {
            if let Ok(rate_info) =
                get_exchange_rate_and_currency(executor, base_ref, target_currency).await
            {
                rates.insert(base_ref.to_string(), rate_info);
            }
        }
    }
    rates
}
