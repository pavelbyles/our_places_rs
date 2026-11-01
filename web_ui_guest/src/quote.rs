use chrono::NaiveDate;
use common::models::ListingDetails;
use rust_decimal::Decimal;
use topcoat::context::Cx;
use web_client::get_api_client;
use web_ui_common::components::domain::price_breakdown::PriceBreakdown;

/// Jamaican GCT on accommodation and cleaning.
pub const GCT_RATE: Decimal = Decimal::from_parts(15, 0, 0, false, 2);

/// Builds the stay price shown on listing quotes, with the legacy portal's
/// arithmetic: GCT applies to the nights plus cleaning; the refundable
/// deposit is added after tax.
pub fn stay_price(
    nights: i64,
    nightly_rate: Decimal,
    subtotal: Decimal,
    cleaning_fee: Option<Decimal>,
    damage_deposit: Option<Decimal>,
    currency: &str,
) -> PriceBreakdown {
    let taxable = subtotal + cleaning_fee.unwrap_or(Decimal::ZERO);
    let tax = taxable * GCT_RATE;
    PriceBreakdown {
        nights,
        effective_nightly_rate: nightly_rate,
        subtotal,
        discount_amount: None,
        cleaning_fee,
        damage_deposit,
        tax_amount: tax,
        total_amount: taxable + tax + damage_deposit.unwrap_or(Decimal::ZERO),
        currency: currency.to_string(),
    }
}

/// Prices a stay with `booking_api`'s dynamic (seasonal) quote. `None` when
/// the API cannot quote it (e.g. below the minimum stay).
pub async fn quote_for(
    cx: &Cx,
    details: &ListingDetails,
    check_in: NaiveDate,
    check_out: NaiveDate,
    currency: Option<&str>,
) -> Option<PriceBreakdown> {
    let quote = get_api_client(cx)
        .get_pricing_quote(details.listing.id, check_in, check_out, currency)
        .await
        .map_err(|e| tracing::info!("no quote for {check_in}..{check_out}: {e}"))
        .ok()?;
    let nights = i64::try_from(quote.nightly_breakdown.len()).ok()?;
    let cur = currency
        .map(str::to_string)
        .unwrap_or_else(|| details.listing.base_currency.clone());
    Some(stay_price(
        nights,
        quote.effective_daily_rate,
        quote.subtotal,
        details.listing.cleaning_fee,
        details.listing.damage_deposit,
        &cur,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn gct_covers_nights_and_cleaning_but_not_the_deposit() {
        let p = stay_price(
            5,
            dec!(650),
            dec!(3250),
            Some(dec!(75)),
            Some(dec!(150)),
            "USD",
        );
        assert_eq!(p.tax_amount, dec!(498.75));
        assert_eq!(p.total_amount, dec!(3973.75));
        assert_eq!(GCT_RATE, dec!(0.15));
    }

    #[test]
    fn missing_fees_count_as_zero() {
        let p = stay_price(2, dec!(100), dec!(200), None, None, "JMD");
        assert_eq!((p.tax_amount, p.total_amount), (dec!(30.00), dec!(230.00)));
    }
}
