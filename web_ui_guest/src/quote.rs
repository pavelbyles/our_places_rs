//! Stay prices from `booking_api`, so the listing page, checkout and the hold
//! all show the same total.

use chrono::NaiveDate;
use common::models::{StayQuoteRequest, StayQuoteResponse};
use common::promo::PromoRefusal;
use rust_decimal::Decimal;
use topcoat::context::Cx;
use uuid::Uuid;
use web_client::get_api_client;
use web_ui_common::components::domain::price_breakdown::PriceBreakdown;

/// The figures for the breakdown panel. `None` for amounts that are zero, so
/// their rows don't show.
pub fn breakdown(quote: &StayQuoteResponse) -> PriceBreakdown {
    let positive = |amount: Decimal| Some(amount).filter(|a| *a > Decimal::ZERO);
    PriceBreakdown {
        nights: i64::from(quote.nights),
        effective_nightly_rate: quote.effective_nightly_rate,
        subtotal: quote.subtotal,
        discount_amount: positive(quote.stay_discount),
        promo_code: quote.promo.as_ref().map(|promo| promo.code.clone()),
        promo_discount: quote.promo.as_ref().map(|promo| promo.discount),
        cleaning_fee: positive(quote.cleaning_fee),
        damage_deposit: positive(quote.damage_deposit),
        platform_fee: positive(quote.platform_fee),
        tax_amount: quote.tax_amount,
        total_amount: quote.total_amount,
        currency: quote.currency.clone(),
    }
}

/// The price of a stay for anyone browsing a listing. `None` when the API can't
/// quote it, such as below the minimum stay: no price is shown, never a guess.
pub async fn public_quote(
    cx: &Cx,
    listing_id: Uuid,
    check_in: NaiveDate,
    check_out: NaiveDate,
    guests: u32,
    currency: Option<&str>,
) -> Option<PriceBreakdown> {
    get_api_client(cx)
        .get_public_stay_quote(listing_id, check_in, check_out, guests, currency)
        .await
        .map_err(|e| tracing::info!("no quote for {check_in}..{check_out}: {e}"))
        .ok()
        .map(|quote| breakdown(&quote))
}

/// A signed-in guest's stay price, with why any promo code was refused. `None`
/// when the API can't quote it.
pub async fn signed_in_quote(
    cx: &Cx,
    token: &str,
    request: &StayQuoteRequest,
) -> Option<(PriceBreakdown, Option<String>)> {
    let quote = get_api_client(cx)
        .get_stay_quote(request, token)
        .await
        .map_err(|e| {
            tracing::warn!(
                listing_id = %request.listing_id,
                "no checkout quote: {e}"
            )
        })
        .ok()?;
    let refusal = quote.promo_refusal.as_ref().map(|refusal| {
        PromoRefusal::from_code(&refusal.code)
            .map(PromoRefusal::message)
            .unwrap_or_else(|| refusal.message.clone())
    });
    Some((breakdown(&quote), refusal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::models::{StayQuotePromo, StayQuoteRefusal};
    use rust_decimal_macros::dec;

    fn quote() -> StayQuoteResponse {
        StayQuoteResponse {
            nights: 7,
            currency: "USD".to_string(),
            effective_nightly_rate: dec!(142.86),
            subtotal: dec!(1000),
            stay_discount: dec!(100),
            promo: Some(StayQuotePromo {
                code: "SUMMER".to_string(),
                discount: dec!(100),
            }),
            promo_refusal: None,
            cleaning_fee: dec!(100),
            tax_amount: dec!(135),
            platform_fee: dec!(40.50),
            damage_deposit: dec!(200),
            total_amount: dec!(1275.50),
        }
    }

    #[test]
    fn the_breakdown_keeps_every_figure() {
        let b = breakdown(&quote());
        assert_eq!(b.nights, 7);
        assert_eq!(b.discount_amount, Some(dec!(100)));
        assert_eq!(b.promo_code.as_deref(), Some("SUMMER"));
        assert_eq!(b.promo_discount, Some(dec!(100)));
        assert_eq!(b.cleaning_fee, Some(dec!(100)));
        assert_eq!(b.platform_fee, Some(dec!(40.50)));
        assert_eq!(b.damage_deposit, Some(dec!(200)));
        assert_eq!(b.tax_amount, dec!(135));
        assert_eq!(b.total_amount, dec!(1275.50));
    }

    #[test]
    fn zero_amounts_have_no_row() {
        let b = breakdown(&StayQuoteResponse {
            stay_discount: Decimal::ZERO,
            promo: None,
            promo_refusal: Some(StayQuoteRefusal {
                code: "PROMO_EXPIRED".to_string(),
                message: "expired".to_string(),
            }),
            cleaning_fee: Decimal::ZERO,
            platform_fee: Decimal::ZERO,
            damage_deposit: Decimal::ZERO,
            ..quote()
        });
        assert_eq!(b.discount_amount, None);
        assert_eq!(b.promo_code, None);
        assert_eq!(b.promo_discount, None);
        assert_eq!(b.cleaning_fee, None);
        assert_eq!(b.platform_fee, None);
        assert_eq!(b.damage_deposit, None);
    }
}
