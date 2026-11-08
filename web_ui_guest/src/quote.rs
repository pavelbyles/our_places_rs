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

/// Why a stay has no price.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoQuote {
    /// `booking_api` refused the stay as asked; the message tells the guest
    /// why and what to change, e.g. "These dates need a stay of at least 5
    /// nights."
    Refused(String),
    /// The quote failed for another reason, such as the API being down.
    Failed,
}

impl NoQuote {
    /// What the guest is told. Only a failure says to try again: retrying
    /// never fixes a refusal.
    pub fn message(&self) -> String {
        match self {
            Self::Refused(message) => message.clone(),
            Self::Failed => {
                "We couldn't price this stay right now. Please try again shortly.".to_string()
            }
        }
    }

    /// Sorts a failed quote by its error text, logging it: a refusal is the
    /// guest's input, a failure is ours.
    fn from_error(e: &anyhow::Error, listing_id: Uuid) -> Self {
        let text = e.to_string();
        match common::availability::stay_refusal_message_in_error(&text) {
            Some(message) => {
                tracing::info!(%listing_id, "stay not quoted: {text}");
                Self::Refused(message)
            }
            None => {
                tracing::warn!(%listing_id, "no stay quote: {text}");
                Self::Failed
            }
        }
    }
}

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
        .map_err(|e| NoQuote::from_error(&e, listing_id))
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
        .map_err(|e| NoQuote::from_error(&e, request.listing_id))?;
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
    fn a_refused_stay_says_why_and_only_a_failure_says_try_again() {
        let id = Uuid::now_v7();
        let short = anyhow::anyhow!(
            r#"Failed to quote stay (400 Bad Request): {{"code":"MINIMUM_STAY_NOT_MET","message":"These dates need a stay of at least 5 nights.","required_nights":5}}"#
        );
        let refused = NoQuote::from_error(&short, id);
        assert_eq!(
            refused,
            NoQuote::Refused("These dates need a stay of at least 5 nights.".to_string())
        );
        assert!(!refused.message().contains("try again"));

        let down =
            NoQuote::from_error(&anyhow::anyhow!("Failed to connect to booking service"), id);
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
