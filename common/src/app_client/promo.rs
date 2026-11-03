//! Stay quotes, and reading promo refusals from API errors.

use super::{booking_api_audience, booking_api_url, get_client};
use crate::models::{StayQuoteRequest, StayQuoteResponse};
use crate::promo::PromoRefusal;
use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use uuid::Uuid;

async fn decode_quote(res: reqwest::Response) -> Result<StayQuoteResponse> {
    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to quote stay ({}): {}", status, err_text);
    }
    res.json()
        .await
        .context("Failed to read the stay quote from the booking service")
}

/// What a stay costs, for anyone browsing a listing. Takes no promo code.
pub async fn get_public_stay_quote(
    listing_id: Uuid,
    check_in: NaiveDate,
    check_out: NaiveDate,
    guests: u32,
    currency: Option<&str>,
) -> Result<StayQuoteResponse> {
    let url = format!(
        "{}/api/v1/bookings/listing/{listing_id}/quote",
        booking_api_url()
    );
    let mut query = vec![
        ("check_in", check_in.to_string()),
        ("check_out", check_out.to_string()),
        ("guests", guests.to_string()),
    ];
    if let Some(currency) = currency {
        query.push(("currency", currency.to_string()));
    }
    let res = get_client()
        .user_request(reqwest::Method::GET, &url, &booking_api_audience(), None)
        .await?
        .query(&query)
        .send()
        .await
        .context("Failed to connect to booking service")?;
    decode_quote(res).await
}

/// What a stay costs the guest who owns `user_jwt`, with any promo code. A
/// refused code is in the response, not an error.
pub async fn get_stay_quote(req: &StayQuoteRequest, user_jwt: &str) -> Result<StayQuoteResponse> {
    let url = format!("{}/api/v1/bookings/quote", booking_api_url());
    let res = get_client()
        .user_request(
            reqwest::Method::POST,
            &url,
            &booking_api_audience(),
            Some(user_jwt),
        )
        .await?
        .json(req)
        .send()
        .await
        .context("Failed to connect to booking service")?;
    decode_quote(res).await
}

/// The promo refusal in an API error's text, if it is one. `FX_RATE_UNAVAILABLE`
/// doesn't count, because the portal already handles it as a currency problem.
pub fn promo_refusal_in_error(text: &str) -> Option<PromoRefusal> {
    PromoRefusal::ALL
        .into_iter()
        .filter(|refusal| refusal.code().starts_with("PROMO_"))
        .find(|refusal| text.contains(refusal.code()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_error_keeps_its_refusal() {
        let text = r#"Failed to create booking (422 Unprocessable Entity): {"code":"PROMO_EXPIRED","message":"That promo code has expired."}"#;
        assert_eq!(promo_refusal_in_error(text), Some(PromoRefusal::Expired));
    }

    #[test]
    fn every_promo_refusal_is_found_in_its_error_body() {
        for refusal in PromoRefusal::ALL {
            let text = format!(r#"{{"code":"{}"}}"#, refusal.code());
            let found = promo_refusal_in_error(&text);
            if refusal == PromoRefusal::FxRateUnavailable {
                assert_eq!(found, None);
            } else {
                assert_eq!(found, Some(refusal), "{refusal:?}");
            }
        }
    }

    #[test]
    fn other_errors_are_not_refusals() {
        for text in [
            r#"{"code":"DATE_UNAVAILABLE"}"#,
            r#"{"code":"FX_RATE_UNAVAILABLE"}"#,
            "connection refused",
        ] {
            assert_eq!(promo_refusal_in_error(text), None, "{text}");
        }
    }
}
