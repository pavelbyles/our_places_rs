//! Stay prices from `booking_api`, so the listing page, checkout and the hold
//! all show the same total.

use chrono::NaiveDate;
use common::models::StayQuoteRequest;
use common::promo::PromoRefusal;
use topcoat::context::Cx;
use uuid::Uuid;
use web_client::get_api_client;
pub use web_ui_common::components::domain::price_breakdown::{NoQuote, PriceBreakdown, breakdown};

/// The price of a stay for anyone browsing a listing, or why there is none,
/// such as below the minimum stay: no price is shown, never a guess.
pub async fn public_quote(
    cx: &Cx,
    listing_id: Uuid,
    check_in: NaiveDate,
    check_out: NaiveDate,
    guests: u32,
    currency: Option<&str>,
) -> Result<PriceBreakdown, NoQuote> {
    get_api_client(cx)
        .get_public_stay_quote(listing_id, check_in, check_out, guests, currency)
        .await
        .map(|quote| breakdown(&quote))
        .map_err(|e| NoQuote::from_error(&e, &listing_id))
}

/// A signed-in guest's stay price, with why any promo code was refused, or why
/// the stay has no price.
pub async fn signed_in_quote(
    cx: &Cx,
    token: &str,
    request: &StayQuoteRequest,
) -> Result<(PriceBreakdown, Option<String>), NoQuote> {
    let quote = get_api_client(cx)
        .get_stay_quote(request, token)
        .await
        .map_err(|e| NoQuote::from_error(&e, &request.listing_id))?;
    let refusal = quote.promo_refusal.as_ref().map(|refusal| {
        PromoRefusal::from_code(&refusal.code)
            .map(PromoRefusal::message)
            .unwrap_or_else(|| refusal.message.clone())
    });
    Ok((breakdown(&quote), refusal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::models::{StayQuotePromo, StayQuoteRefusal, StayQuoteResponse};
    use rust_decimal::Decimal;
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
    fn a_refused_stay_says_why_and_only_a_failure_says_try_again() {
        let id = Uuid::now_v7();
        let short = anyhow::anyhow!(
            r#"Failed to quote stay (400 Bad Request): {{"code":"MINIMUM_STAY_NOT_MET","message":"These dates need a stay of at least 5 nights.","required_nights":5}}"#
        );
        let refused = NoQuote::from_error(&short, &id);
        assert_eq!(
            refused,
            NoQuote::Refused("These dates need a stay of at least 5 nights.".to_string())
        );
        assert!(!refused.message().contains("try again"));

        let down = NoQuote::from_error(
            &anyhow::anyhow!("Failed to connect to booking service"),
            &id,
        );
        assert_eq!(down, NoQuote::Failed);
        assert!(down.message().contains("try again"));
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
