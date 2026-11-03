//! Prices a stay. Quotes and holds share this, so they can't disagree.
//!
//! Promo code strings never enter a log or span field: only a hash of them does.

use super::{map_pricing_error, nightly_price};
use actix_web::{HttpResponse, web};
use api_core::auth::Claims;
use api_core::error::ApiError;
use api_core::settings::Settings;
use chrono::{NaiveDate, Utc};
use common::availability::{StayInputError, validate_stay_dates};
use common::currency::CurrencyCode;
use common::models::{StayQuotePromo, StayQuoteRefusal, StayQuoteRequest, StayQuoteResponse};
use common::pricing::{
    BookingCalculator, StayPricingInput, Taxed, fits_booking_columns, price_before_promo,
};
use common::promo::{
    PromoRefusal, PromoStay, PromoTerms, check_status, check_stay_rules, check_usage,
    normalize_code,
};
use db_core::error::DbError;
use db_core::exchange_rate::{RateUse, get_rate_pool};
use db_core::listing as db_listing;
use db_core::models::{Listing, NewBookingPromo};
use db_core::promo::PromoCode;
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

/// What to do when a code can't be used.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OnRefusal {
    /// Fail with the reason: a hold must not continue at a different price.
    Fail,
    /// Price without the code and report why: a quote shows the full price.
    Report,
}

/// A code to apply to a stay.
#[derive(Clone, Copy)]
pub(super) struct PromoRequest<'a> {
    /// The code as typed.
    pub(super) raw_code: &'a str,
    /// The guest it's for.
    pub(super) guest_id: Uuid,
    /// What to do when it can't be used.
    pub(super) on_refusal: OnRefusal,
}

/// The stay to price.
pub(super) struct StayRequest<'a> {
    /// The listing being booked.
    pub(super) listing: &'a Listing,
    /// First night.
    pub(super) check_in: NaiveDate,
    /// Departure date.
    pub(super) check_out: NaiveDate,
    /// Guests staying.
    pub(super) number_of_persons: i32,
    /// Requested payment currency, as sent.
    pub(super) currency: &'a str,
    /// The code to apply, if any.
    pub(super) promo: Option<PromoRequest<'a>>,
    /// Longest stay that may be priced.
    pub(super) max_stay_nights: i64,
    /// Today in Jamaica: earlier check-ins are refused.
    pub(super) today: NaiveDate,
}

/// Today's date for a stay being priced now.
pub(super) fn stay_today() -> NaiveDate {
    common::availability::jamaica_today(Utc::now())
}

/// A priced stay.
pub(super) struct PricedStay {
    /// The finished calculation.
    pub(super) calculator: BookingCalculator<Taxed>,
    /// Listing currency to payment currency.
    pub(super) conversion_rate: Decimal,
    /// The payment currency.
    pub(super) currency: String,
    /// The code being redeemed, if it can be used.
    pub(super) promo: Option<NewBookingPromo>,
    /// Why the code couldn't be used, when asked to report it.
    pub(super) promo_refusal: Option<PromoRefusal>,
}

fn bad_request(message: String) -> ApiError {
    ApiError::Database(DbError::ValidationError(message))
}

/// Records a refused code for spotting guessing, with a hash, never the code.
pub(super) fn log_promo_refusal(guest_id: Uuid, refusal: PromoRefusal, raw_code: &str) {
    let normalized = normalize_code(raw_code).ok().flatten();
    let hashed = normalized.as_deref().unwrap_or_else(|| raw_code.trim());
    let code_hash: String = Sha256::digest(hashed.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    tracing::warn!(%guest_id, reason = refusal.code(), %code_hash, "promo code refused");
}

/// Whether a code can be used, and its terms if so. Follows the documented
/// check order, so a code failing several checks gives a predictable reason.
/// At a hold, status and usage are checked again under the code's lock.
async fn resolve_promo(
    pool: &PgPool,
    code: &str,
    listing_id: Uuid,
    promo: PromoRequest<'_>,
    payment_currency: &str,
    stay: PromoStay,
    amounts: (Decimal, Decimal),
) -> Result<Result<(PromoCode, PromoTerms), PromoRefusal>, ApiError> {
    let now = Utc::now();
    let Some(found) = db_core::promo::get_by_code(pool, code).await? else {
        return Ok(Err(PromoRefusal::NotFound));
    };
    if let Err(refusal) = check_status(found.status(), now) {
        return Ok(Err(refusal));
    }
    if !db_core::promo::covers_listing(pool, &found, listing_id).await? {
        return Ok(Err(PromoRefusal::ListingNotEligible));
    }
    let rate = match found.currency.as_deref().map(str::trim) {
        None => Decimal::ONE,
        Some(currency) => {
            match get_rate_pool(pool, currency, payment_currency, now, RateUse::NewQuote).await {
                Ok(rate) => rate,
                Err(
                    DbError::FxRateUnavailable { .. }
                    | DbError::CurrencyInactive(_)
                    | DbError::CurrencyUnknown(_),
                ) => return Ok(Err(PromoRefusal::FxRateUnavailable)),
                Err(e) => return Err(ApiError::Database(e)),
            }
        }
    };
    let terms = found.terms(rate);
    let eligible = terms.eligible_amount(amounts.0, amounts.1);
    if let Err(refusal) = check_stay_rules(&terms, stay, eligible) {
        return Ok(Err(refusal));
    }
    if promo.on_refusal == OnRefusal::Report {
        let usage = db_core::promo::usage_for_quote(pool, &found, promo.guest_id).await?;
        if let Err(refusal) = check_usage(usage) {
            return Ok(Err(refusal));
        }
    }
    Ok(Ok((found, terms)))
}

/// Refuses a stay whose amounts wouldn't fit the booking's money columns, rather
/// than quoting a price the hold can't store.
fn check_amounts_fit(calculator: &BookingCalculator<Taxed>) -> Result<(), ApiError> {
    if !fits_booking_columns(calculator) {
        return Err(bad_request(
            "This stay's total is too large to book online. Please choose fewer nights."
                .to_string(),
        ));
    }
    Ok(())
}

/// Prices a stay in the requested currency, with any promo code.
pub(super) async fn price_stay(
    pool: &PgPool,
    request: StayRequest<'_>,
) -> Result<PricedStay, ApiError> {
    let listing = request.listing;
    validate_stay_dates(
        request.check_in,
        request.check_out,
        request.today,
        request.max_stay_nights,
    )
    .map_err(|refused| {
        tracing::warn!(listing_id = %listing.id, code = refused.code(), "stay dates refused");
        ApiError::Database(DbError::StayInput(refused))
    })?;
    // `max_stay_nights` has no upper bound, and the calculator counts in `i32`.
    i32::try_from((request.check_out - request.check_in).num_days()).map_err(|_| {
        ApiError::Database(DbError::StayInput(StayInputError::TooLong {
            max_nights: request.max_stay_nights,
        }))
    })?;

    let currency = request.currency.trim().to_ascii_uppercase();
    CurrencyCode::try_from(currency.as_str()).map_err(|e| {
        tracing::warn!(error = %e, "rejected malformed checkout currency");
        bad_request(e.to_string())
    })?;

    // Every amount is priced in the checkout currency. A missing rate is a 422,
    // never a silent fallback that would label base-currency amounts wrongly.
    let conversion_rate = get_rate_pool(
        pool,
        &listing.base_currency,
        &currency,
        Utc::now(),
        RateUse::NewQuote,
    )
    .await
    .map_err(|e| {
        // A warning: visitors can ask for any currency on the public quote.
        tracing::warn!(
            listing_id = %listing.id,
            from = %listing.base_currency,
            to = %currency,
            error = %e,
            "checkout currency conversion failed"
        );
        ApiError::Database(e)
    })?;
    let raw_overrides = db_listing::get_active_overrides_for_dates(
        pool,
        listing.id,
        request.check_in,
        request.check_out,
    )
    .await
    .map_err(|e| {
        tracing::error!(listing_id = %listing.id, error = %e, "failed to load price overrides for checkout");
        ApiError::Database(e)
    })?;
    let discounted = price_before_promo(StayPricingInput {
        base_nightly_rate: nightly_price(listing)?,
        minimum_stay: listing.minimum_stay,
        overrides: &raw_overrides,
        rate: conversion_rate,
        check_in: request.check_in,
        check_out: request.check_out,
        number_of_persons: request.number_of_persons,
        guests_included: listing.guests_included,
        extra_guest_fee: listing.extra_guest_fee,
        cleaning_fee: listing.cleaning_fee,
        damage_deposit: listing.damage_deposit,
        weekly_discount_percentage: listing.weekly_discount_percentage,
        monthly_discount_percentage: listing.monthly_discount_percentage,
    })
    .map_err(map_pricing_error)?;

    let mut resolved = None;
    let mut promo_refusal = None;
    let requested = match request.promo {
        Some(promo) => match normalize_code(promo.raw_code) {
            Ok(Some(code)) => Some((promo, Ok(code))),
            Ok(None) => None,
            Err(refusal) => Some((promo, Err(refusal))),
        },
        None => None,
    };
    if let Some((promo, code)) = requested {
        let outcome = match code {
            Ok(code) => {
                resolve_promo(
                    pool,
                    &code,
                    listing.id,
                    promo,
                    &currency,
                    PromoStay {
                        check_in: request.check_in,
                        check_out: request.check_out,
                    },
                    (discounted.discounted_subtotal, discounted.cleaning_fee),
                )
                .await?
            }
            Err(refusal) => Err(refusal),
        };
        match outcome {
            Ok(found) => resolved = Some(found),
            Err(refusal) => {
                log_promo_refusal(promo.guest_id, refusal, promo.raw_code);
                if promo.on_refusal == OnRefusal::Fail {
                    return Err(ApiError::Database(DbError::PromoRefused(refusal)));
                }
                promo_refusal = Some(refusal);
            }
        }
    }

    let calculator = discounted
        .apply_promo(resolved.as_ref().map(|(_, terms)| terms))
        .apply_taxes(common::reference::SupportedCountry::tax_rate_for(
            &listing.country,
        ))
        .finalize();
    check_amounts_fit(&calculator)?;

    let promo = resolved.map(|(found, terms)| NewBookingPromo {
        promo_code_id: found.id,
        terms,
        stay_discount: calculator.promo_stay_discount,
        cleaning_discount: calculator.promo_cleaning_discount,
    });

    Ok(PricedStay {
        calculator,
        conversion_rate,
        currency,
        promo,
        promo_refusal,
    })
}

/// Checks the guest count a stay is priced for.
pub(super) fn check_guest_count(listing: &Listing, number_of_persons: i32) -> Result<(), ApiError> {
    if number_of_persons > listing.max_guests {
        return Err(bad_request(format!(
            "Number of guests ({}) exceeds listing max_guests ({})",
            number_of_persons, listing.max_guests
        )));
    }
    Ok(())
}

/// Loads the listing a new stay is quoted for. Inactive listings are refused.
pub(super) async fn load_listing(pool: &PgPool, listing_id: Uuid) -> Result<Listing, ApiError> {
    db_listing::get_listing_by_id(pool, listing_id)
        .await
        .map(|details| details.listing)
        .map_err(|e| {
            if let DbError::Sqlx(sqlx::Error::RowNotFound) = e {
                bad_request("Listing not found".to_string())
            } else {
                ApiError::Database(e)
            }
        })
        .and_then(|listing| {
            if listing.is_active {
                Ok(listing)
            } else {
                Err(ApiError::Database(DbError::ListingUnavailable))
            }
        })
}

fn quote_response(priced: PricedStay) -> StayQuoteResponse {
    let c = &priced.calculator;
    StayQuoteResponse {
        nights: c.total_days,
        currency: priced.currency.clone(),
        effective_nightly_rate: c.actual_daily_rate,
        subtotal: c.sub_total_price,
        stay_discount: c.discount_value.unwrap_or(Decimal::ZERO),
        promo: priced.promo.as_ref().map(|promo| StayQuotePromo {
            code: promo.terms.code.clone(),
            discount: promo.stay_discount + promo.cleaning_discount,
        }),
        promo_refusal: priced.promo_refusal.map(|refusal| StayQuoteRefusal {
            code: refusal.code().to_string(),
            message: refusal.message(),
        }),
        cleaning_fee: c.cleaning_fee,
        tax_amount: c.tax_value.unwrap_or(Decimal::ZERO),
        platform_fee: c.platform_fee,
        damage_deposit: c.damage_deposit,
        total_amount: c.total_price,
    }
}

/// Query for the public listing quote.
#[derive(serde::Deserialize, utoipa::IntoParams)]
pub(super) struct PublicQuoteQuery {
    /// First night.
    check_in: NaiveDate,
    /// Departure date.
    check_out: NaiveDate,
    /// Guests staying. Defaults to one.
    guests: Option<u32>,
    /// Payment currency. Defaults to the listing's.
    currency: Option<String>,
}

/// What a stay costs, for anyone browsing a listing. Takes no promo code, so
/// codes can't be tested signed out.
#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/bookings/listing/{id}/quote",
    tag = "bookings",
    params(PublicQuoteQuery),
    responses(
        (status = 200, description = "The stay's price", body = StayQuoteResponse),
        (status = 400, description = "Invalid stay, too many guests, or a stay over the limit"),
        (status = 422, description = "FX_RATE_UNAVAILABLE")
    )
)]
pub(super) async fn public_listing_quote(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    id: web::Path<Uuid>,
    query: web::Query<PublicQuoteQuery>,
) -> Result<HttpResponse, ApiError> {
    let listing = load_listing(pool.get_ref(), *id).await?;
    let guests = i32::try_from(query.guests.unwrap_or(1).max(1))
        .map_err(|_| bad_request("Invalid number of guests".to_string()))?;
    check_guest_count(&listing, guests)?;
    let priced = price_stay(
        pool.get_ref(),
        StayRequest {
            listing: &listing,
            check_in: query.check_in,
            check_out: query.check_out,
            number_of_persons: guests,
            currency: query.currency.as_deref().unwrap_or(&listing.base_currency),
            promo: None,
            max_stay_nights: settings.application.max_stay_nights,
            today: stay_today(),
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(quote_response(priced)))
}

/// What a stay costs a signed-in guest, with any promo code. A refused code
/// returns the full price and the reason.
#[tracing::instrument(skip_all, fields(guest_id = %claims.sub))]
#[utoipa::path(
    post,
    path = "/api/v1/bookings/quote",
    tag = "bookings",
    request_body = StayQuoteRequest,
    responses(
        (status = 200, description = "The stay's price, with the discount or the reason a code was refused", body = StayQuoteResponse),
        (status = 400, description = "Invalid stay, too many guests, or a stay over the limit"),
        (status = 401, description = "Not signed in"),
        (status = 422, description = "FX_RATE_UNAVAILABLE for the payment currency")
    )
)]
pub(super) async fn signed_in_quote(
    claims: Claims,
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    body: web::Json<StayQuoteRequest>,
) -> Result<HttpResponse, ApiError> {
    body.validate().map_err(ApiError::ValidationError)?;
    let guest = super::caller(pool.get_ref(), &claims).await?;
    let listing = load_listing(pool.get_ref(), body.listing_id).await?;
    let guests = i32::try_from(body.guests.max(1))
        .map_err(|_| bad_request("Invalid number of guests".to_string()))?;
    check_guest_count(&listing, guests)?;
    let priced = price_stay(
        pool.get_ref(),
        StayRequest {
            listing: &listing,
            check_in: body.check_in,
            check_out: body.check_out,
            number_of_persons: guests,
            currency: body.currency.as_deref().unwrap_or(&listing.base_currency),
            promo: body.promo_code.as_deref().map(|raw_code| PromoRequest {
                raw_code,
                guest_id: guest.id,
                on_refusal: OnRefusal::Report,
            }),
            max_stay_nights: settings.application.max_stay_nights,
            today: stay_today(),
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(quote_response(priced)))
}
