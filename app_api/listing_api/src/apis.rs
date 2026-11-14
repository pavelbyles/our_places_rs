use crate::authz;
use actix_web::middleware::from_fn;
use actix_web::{HttpRequest, HttpResponse, Responder, web};
use api_core::api_common::content_negotiation_middleware;
use api_core::auth::Claims;
use api_core::models::{
    ListingsWrapper, map_listing_to_response, map_listing_with_owner_to_response,
};
use api_core::response::{Payload, respond};
use api_core::{error::ApiError, pagination, settings::Settings};
use chrono::NaiveDate;
use common::availability::{StaySearch, jamaica_today, parse_stay_search};
use common::currency::CurrencyCode;
use common::models::{
    ListingQueryParams, ListingResponse, PriceOverride, StayTotal, UpdatedListingRequest,
};
use common::pricing::{StayPricingInput, fits_booking_columns, price_before_promo};
use common::reference::SupportedCountry;
use db_core::listing as db_listing;
use db_core::models::{ListingWithOwner, NewListing, StructureType, UpdatedListing};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::HashMap;
use std::str::FromStr;
use utoipa::{IntoParams, OpenApi};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
use validator::Validate;

/// Parses an optional `?currency=` value strictly: a malformed code is a 400,
/// never silently ignored or normalised.
fn requested_currency(raw: Option<&str>) -> Result<Option<CurrencyCode>, ApiError> {
    raw.map(|code| {
        CurrencyCode::try_from(code).map_err(|e| {
            tracing::warn!(currency = code, "rejected malformed currency parameter");
            ApiError::Database(db_core::error::DbError::ValidationError(e.to_string()))
        })
    })
    .transpose()
}

/// Loads the rates for a search in one batched lookup. Returns before pricing
/// starts, so no connection is held during it.
async fn load_search_rates(
    pool: &PgPool,
    bases: &[String],
    target: &CurrencyCode,
) -> Result<HashMap<String, Decimal>, ApiError> {
    let mut conn = pool.acquire().await.map_err(|e| {
        tracing::error!(error = %e, "could not acquire a connection for search conversion");
        ApiError::Database(e.into())
    })?;
    db_core::exchange_rate::get_rates_for_bases(
        &mut conn,
        bases,
        target.as_str(),
        chrono::Utc::now(),
        db_core::exchange_rate::RateUse::NewQuote,
    )
    .await
    .map_err(|e| {
        tracing::warn!(to = %target, error = %e, "search currency conversion failed");
        ApiError::Database(e)
    })
}

/// Converts search results into `target` using rates already loaded.
///
/// Listings whose base currency cannot be quoted in `target` keep their base
/// currency label, lose their prices and are flagged with
/// `price_unavailable_in`: no fallback rate, no relabelled amounts. One
/// aggregated `warn!` names the missing pairs.
fn convert_search_prices(
    listings: &mut [ListingResponse],
    rates: &HashMap<String, Decimal>,
    target: &CurrencyCode,
) {
    let missing: std::collections::BTreeSet<String> = listings
        .iter_mut()
        .filter_map(|listing| match rates.get(&listing.base_currency) {
            Some(rate) => {
                listing.price_per_night = listing.price_per_night.map(|p| (p * rate).round_dp(2));
                listing.extra_guest_fee = listing.extra_guest_fee.map(|p| (p * rate).round_dp(2));
                listing.base_currency = target.to_string();
                None
            }
            None => {
                listing.price_per_night = None;
                listing.extra_guest_fee = None;
                listing.price_unavailable_in = Some(target.to_string());
                Some(listing.base_currency.clone())
            }
        })
        .collect();
    if !missing.is_empty() {
        let affected = listings
            .iter()
            .filter(|l| l.price_unavailable_in.is_some())
            .count();
        tracing::warn!(
            to = %target,
            missing_bases = ?missing,
            affected,
            "search prices unavailable in requested currency"
        );
    }
}

/// One listing's stay to price. Owned, so it can move to a blocking thread.
struct StayToPrice {
    listing_id: Uuid,
    nightly: Option<Decimal>,
    minimum_stay: i32,
    guests_included: i32,
    extra_guest_fee: Decimal,
    cleaning_fee: Decimal,
    damage_deposit: Decimal,
    weekly_discount_percentage: Option<Decimal>,
    monthly_discount_percentage: Option<Decimal>,
    country: String,
    overrides: Vec<PriceOverride>,
    rate: Decimal,
    currency: String,
}

/// What the stay is, for every listing in a search.
#[derive(Clone, Copy)]
struct SearchedStay {
    check_in: NaiveDate,
    check_out: NaiveDate,
    guests: i32,
}

/// Search totals, and the listings that couldn't be priced.
#[derive(Default)]
struct PricedStays {
    totals: HashMap<Uuid, StayTotal>,
    failed: Vec<Uuid>,
}

/// A stored discount outside 0–100 could overflow the calculation. New values
/// are validated, but older rows may hold anything.
fn discount_in_range(pct: Option<Decimal>) -> bool {
    pct.is_none_or(|pct| (Decimal::ZERO..=Decimal::ONE_HUNDRED).contains(&pct))
}

/// A stay's total, through the same calculation as a quote and a hold. `None`
/// when it can't be priced.
fn price_search_stay(job: &StayToPrice, stay: SearchedStay) -> Option<StayTotal> {
    if !discount_in_range(job.weekly_discount_percentage)
        || !discount_in_range(job.monthly_discount_percentage)
    {
        return None;
    }
    let calculator = price_before_promo(StayPricingInput {
        base_nightly_rate: job.nightly?,
        minimum_stay: job.minimum_stay,
        overrides: &job.overrides,
        rate: job.rate,
        check_in: stay.check_in,
        check_out: stay.check_out,
        number_of_persons: stay.guests,
        guests_included: job.guests_included,
        extra_guest_fee: job.extra_guest_fee,
        cleaning_fee: job.cleaning_fee,
        damage_deposit: job.damage_deposit,
        weekly_discount_percentage: job.weekly_discount_percentage,
        monthly_discount_percentage: job.monthly_discount_percentage,
    })
    .ok()?
    .apply_promo(None)
    .apply_taxes(SupportedCountry::tax_rate_for(&job.country))
    .finalize();
    fits_booking_columns(&calculator).then(|| StayTotal {
        nights: calculator.total_days,
        total_amount: calculator.total_price,
        damage_deposit: calculator.damage_deposit,
        currency: job.currency.clone(),
    })
}

fn price_search_stays(jobs: Vec<StayToPrice>, stay: SearchedStay) -> PricedStays {
    let mut priced = PricedStays::default();
    for job in jobs {
        match price_search_stay(&job, stay) {
            Some(total) => {
                priced.totals.insert(job.listing_id, total);
            }
            None => priced.failed.push(job.listing_id),
        }
    }
    priced
}

/// If the pricing task itself fails, the listings still return, with no totals.
fn totals_or_none(result: Result<PricedStays, tokio::task::JoinError>) -> PricedStays {
    result.unwrap_or_else(|e| {
        tracing::error!(error = %e, "pricing search results failed");
        PricedStays::default()
    })
}

/// Totals for a page of search results. Works from the database rows, because
/// `convert_search_prices` overwrites the response's prices and currency.
///
/// A failed override load fails the request: pricing without overrides would
/// be a guess. A listing with no rate to `target` gets no total, and
/// `convert_search_prices` flags it. The loop runs on a blocking thread, because
/// a stay can be years long and a host sets the number of overrides.
async fn search_stay_totals(
    pool: &PgPool,
    rows: &[ListingWithOwner],
    stay: SearchedStay,
    rates: Option<&HashMap<String, Decimal>>,
    target: Option<&CurrencyCode>,
) -> Result<HashMap<Uuid, StayTotal>, ApiError> {
    let ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
    let mut overrides =
        db_listing::get_active_overrides_for_listings(pool, &ids, stay.check_in, stay.check_out)
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "could not load overrides for search totals");
                ApiError::Database(e)
            })?;
    let jobs: Vec<StayToPrice> = rows
        .iter()
        .filter_map(|row| {
            let rate = match rates {
                Some(rates) => *rates.get(&row.base_currency)?,
                None => Decimal::ONE,
            };
            Some(StayToPrice {
                listing_id: row.id,
                nightly: row.price_per_night,
                minimum_stay: row.minimum_stay,
                guests_included: row.guests_included,
                extra_guest_fee: row.extra_guest_fee,
                cleaning_fee: row.cleaning_fee,
                damage_deposit: row.damage_deposit,
                weekly_discount_percentage: row.weekly_discount_percentage,
                monthly_discount_percentage: row.monthly_discount_percentage,
                country: row.country.clone(),
                overrides: overrides.remove(&row.id).unwrap_or_default(),
                rate,
                currency: target.map_or_else(|| row.base_currency.clone(), ToString::to_string),
            })
        })
        .collect();

    let priced =
        totals_or_none(tokio::task::spawn_blocking(move || price_search_stays(jobs, stay)).await);
    if !priced.failed.is_empty() {
        tracing::warn!(listings = ?priced.failed, "search stays could not be priced");
    }
    Ok(priced.totals)
}

/// Reads what a guest searched for. A refusal is logged by its code alone,
/// because the input can hold what they typed.
fn read_stay_search(
    query: &ListingQueryParams,
    now: chrono::DateTime<chrono::Utc>,
    max_stay_nights: i64,
) -> Result<StaySearch, ApiError> {
    parse_stay_search(
        query.check_in.as_deref(),
        query.check_out.as_deref(),
        query.guests.as_deref(),
        query.q.as_deref(),
        jamaica_today(now),
        max_stay_nights,
    )
    .map_err(|refused| {
        tracing::warn!(code = refused.code(), "search input refused");
        ApiError::Database(db_core::error::DbError::StayInput(refused))
    })
}

/// Gives first 10 listings if no page or per_page is provided
#[utoipa::path(
    get,
    path = "/api/v1/listings",
    tag = "listings",
    params(
        ListingQueryParams
    ),
    responses(
        (status = 200, description = "List of listings", body = [ListingResponse]),
        (status = 400, description = "INVALID_STAY_DATES, INVALID_GUESTS or INVALID_SEARCH_TEXT"),
        (status = 500, description = "Internal server error")
    )
)]
// `skip_all`: the query and request both hold what a guest typed.
#[tracing::instrument(skip_all)]
pub async fn get_listings(
    pool: web::Data<sqlx::PgPool>,
    query: web::Query<common::models::ListingQueryParams>,
    settings: web::Data<Settings>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse, actix_web::Error> {
    tracing::info!("GET /api/v1/listings entered");
    let mut structure_types = Vec::new();
    let qs = req.query_string();

    if let Ok(params) = serde_urlencoded::from_str::<Vec<(String, String)>>(qs) {
        for (key, value) in params {
            if key == "structure_type" {
                for part in value.split(',') {
                    let trimmed = part.trim();
                    if !trimmed.is_empty() {
                        structure_types.push(trimmed.to_string());
                    }
                }
            }
        }
    }

    let search = read_stay_search(
        &query,
        chrono::Utc::now(),
        settings.application.max_stay_nights,
    )?;
    let (check_in, check_out) = search.dates.unzip();

    // Set default values for pagination if they are not provided.
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10);

    // Minimum of per_page or 100
    let per_page_clamped = per_page.min(100);

    let filter = common::models::ListingFilter {
        name: query.name.clone(),
        country: query.country.clone(),
        min_price: query.min_price,
        max_price: query.max_price,
        structure_type: structure_types,
        owner: query.owner.clone(),
        resolution: query.resolution.clone(),
        currency: query.currency.clone(),
        user_id: query.user_id,
        host: query.host.clone(),
        include_inactive: query.include_inactive,
        check_in,
        check_out,
        guests: search.guests,
        q: search.q.clone(),
    };

    let target = requested_currency(query.currency.as_deref())?;
    let rows = db_listing::get_listings(pool.get_ref(), page, per_page_clamped, Some(filter))
        .await
        .map_err(|e| {
            tracing::error!("Database query failed: {:?}", e);
            ApiError::Database(e)
        })?;

    let rates = match &target {
        Some(target) => {
            let bases: Vec<String> = rows
                .iter()
                .map(|row| row.base_currency.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            Some(load_search_rates(pool.get_ref(), &bases, target).await?)
        }
        None => None,
    };

    let mut totals = match search.dates {
        Some((check_in, check_out)) if !rows.is_empty() => {
            let stay = SearchedStay {
                check_in,
                check_out,
                guests: search.guests.unwrap_or(1),
            };
            search_stay_totals(pool.get_ref(), &rows, stay, rates.as_ref(), target.as_ref()).await?
        }
        _ => HashMap::new(),
    };

    let mut response: Vec<ListingResponse> = rows
        .into_iter()
        .map(map_listing_with_owner_to_response)
        .collect();

    if let (Some(rates), Some(target)) = (&rates, &target) {
        convert_search_prices(&mut response, rates, target);
    }
    for listing in &mut response {
        listing.stay_total = totals.remove(&listing.id);
    }

    Ok(respond(
        &req,
        Payload::Collection(response),
        |items| ListingsWrapper { listing: items },
        actix_web::http::StatusCode::OK,
    ))
}

/// A listing as its manager sees it: the stored coordinates and setting.
/// Only call this after `require_listing_manager` or `require_host_or_admin`.
fn manager_listing_response(listing: db_core::models::Listing) -> ListingResponse {
    let stored = api_core::models::StoredLocation::from(&listing);
    let mut response = map_listing_to_response(listing);
    api_core::models::with_stored_location(&mut response, stored);
    response
}

/// Query parameter choosing a display currency.
#[derive(Deserialize, Debug)]
pub struct CurrencyQuery {
    /// Currency to show amounts in.
    pub currency: Option<String>,
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/listings/{id}",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID or slug")
    ),
    responses(
        (status = 200, description = "Listing found", body = common::models::ListingDetails),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_listing_by_id(
    req: HttpRequest,
    path: web::Path<String>,
    pool: web::Data<PgPool>,
    query: web::Query<CurrencyQuery>,
) -> Result<impl Responder, ApiError> {
    let listing_id_or_slug = path.into_inner();
    let listing_details =
        db_listing::get_listing_by_id_or_slug(pool.get_ref(), &listing_id_or_slug).await?;

    let mut response = api_core::models::map_listing_details_to_response(listing_details);

    if let Some(target) = requested_currency(query.currency.as_deref())? {
        let rate = db_core::exchange_rate::get_rate_pool(
            pool.get_ref(),
            &response.listing.base_currency,
            target.as_str(),
            chrono::Utc::now(),
            db_core::exchange_rate::RateUse::NewQuote,
        )
        .await
        .map_err(|e| {
            tracing::warn!(
                listing_id = %response.listing.id,
                from = %response.listing.base_currency,
                to = %target,
                error = %e,
                "listing detail currency conversion failed"
            );
            ApiError::Database(e)
        })?;
        response.listing.price_per_night = response
            .listing
            .price_per_night
            .map(|p| (p * rate).round_dp(2));
        response.listing.extra_guest_fee = response
            .listing
            .extra_guest_fee
            .map(|p| (p * rate).round_dp(2));
        response.listing.base_currency = target.to_string();
    }

    Ok(respond(
        &req,
        Payload::Item(response),
        |_: Vec<common::models::ListingDetails>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    post,
    path = "/api/v1/listings",
    tag = "listings",
    request_body = common::models::NewListingRequest,
    responses(
        (status = 201, description = "Listing created", body = ListingResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid user token"),
        (status = 403, description = "Not the listing's owner or an admin"),
        (status = 500, description = "Internal server error")
    )
)]
async fn create_listing(
    claims: Claims,
    req: HttpRequest,
    pool: web::Data<PgPool>,
    new_listing: web::Json<common::models::NewListingRequest>,
    settings: web::Data<Settings>,
) -> Result<impl Responder, ApiError> {
    let creator = authz::require_host_or_admin(pool.get_ref(), &claims, "create_listing").await?;
    let mut req_data = new_listing.into_inner();
    req_data.validate()?;
    common::location::validate_coordinates(req_data.latitude, req_data.longitude)?;
    // A host always owns what they create; an admin may name an owner.
    req_data.user_id = authz::listing_owner_for(pool.get_ref(), &creator, req_data.user_id).await?;

    if req_data.guests_included > req_data.max_guests {
        let mut errors = validator::ValidationErrors::new();
        errors.add(
            "guests_included",
            validator::ValidationError::new("guests_included_exceeds_max_guests")
                .with_message("guests_included cannot exceed max_guests.".into()),
        );
        return Err(ApiError::ValidationError(errors));
    }

    let structure_type = StructureType::from_str(&req_data.listing_structure).map_err(|_| {
        let mut errors = validator::ValidationErrors::new();
        errors.add(
            "listing_structure",
            validator::ValidationError::new("invalid_structure_type")
                .with_message("Invalid structure type provided.".into()),
        );
        ApiError::ValidationError(errors)
    })?;

    let structure_id = structure_type.id();

    let mut attempts = 0;
    let max_attempts = settings.application.max_attempts;

    loop {
        attempts += 1;
        let listing = NewListing {
            name: req_data.name.clone(),
            user_id: req_data.user_id,
            description: req_data.description.clone(),
            listing_structure_id: structure_id,
            country: req_data.country.clone(),
            price_per_night: req_data.price_per_night,
            weekly_discount_percentage: req_data.weekly_discount_percentage,
            monthly_discount_percentage: req_data.monthly_discount_percentage,
            max_guests: req_data.max_guests,
            bedrooms: req_data.bedrooms,
            beds: req_data.beds,
            full_bathrooms: req_data.full_bathrooms,
            half_bathrooms: req_data.half_bathrooms,
            square_meters: req_data.square_meters,
            latitude: req_data.latitude,
            longitude: req_data.longitude,
            listing_details: req_data.listing_details.clone(),
            city: req_data.city.clone(),
            base_currency: req_data.base_currency.clone(),
            minimum_stay: req_data.minimum_stay,
            days_between_bookings: req_data.days_between_bookings,
            commission_pct: req_data.commission_pct,
            cleaning_fee: req_data.cleaning_fee,
            damage_deposit: req_data.damage_deposit,
            guests_included: Some(req_data.guests_included),
            extra_guest_fee: req_data.extra_guest_fee,
            is_active: req_data.is_active,
            location_privacy: req_data.location_privacy,
        };

        match db_listing::create_listing_with_active_currency(pool.get_ref(), &listing).await {
            Ok(created_listing) => {
                return Ok(respond(
                    &req,
                    Payload::Item(manager_listing_response(created_listing)),
                    |_: Vec<ListingResponse>| (),
                    actix_web::http::StatusCode::CREATED,
                ));
            }
            Err(e) => {
                match e {
                    db_core::error::DbError::ValidationError(msg) => {
                        let mut map = std::collections::HashMap::new();
                        map.insert(
                            std::borrow::Cow::from("validation"),
                            validator::ValidationErrorsKind::Field(vec![
                                validator::ValidationError::new("custom").with_message(msg.into()),
                            ]),
                        );
                        return Err(ApiError::ValidationError(validator::ValidationErrors(map)));
                    }
                    db_core::error::DbError::Sqlx(ref sqlx_error) => {
                        if let Some(db_error) = sqlx_error.as_database_error()
                            && db_error.code().as_deref() == Some("23505")
                            && let Some(constraint) = db_error.constraint()
                            && constraint == "listing_pkey"
                        {
                            if attempts >= max_attempts {
                                return Err(ApiError::Internal);
                            }
                            continue;
                        }
                    }
                    _ => {}
                }
                return Err(ApiError::Database(e));
            }
        }
    }
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    patch,
    path = "/api/v1/listings/{id}",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID")
    ),
    request_body = UpdatedListingRequest,
    responses(
        (status = 200, description = "Listing updated", body = ListingResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid user token"),
        (status = 403, description = "Not the listing's owner or an admin"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_listing(
    claims: Claims,
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    updated_listing_req: web::Json<UpdatedListingRequest>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    authz::require_listing_manager(pool.get_ref(), &claims, listing_id, "update_listing").await?;
    let req_data = updated_listing_req.into_inner();
    req_data.validate()?;
    common::location::validate_coordinates(req_data.latitude, req_data.longitude)?;

    let structure_id = if let Some(ref structure_str) = req_data.listing_structure {
        let st = StructureType::from_str(structure_str).map_err(|_| {
            let mut errors = validator::ValidationErrors::new();
            errors.add(
                "listing_structure",
                validator::ValidationError::new("invalid_structure_type")
                    .with_message("Invalid structure type provided.".into()),
            );
            ApiError::ValidationError(errors)
        })?;
        Some(st.id())
    } else {
        None
    };

    let city = if req_data.city.is_some() {
        req_data.city
    } else if let (Some(lat), Some(lon)) = (req_data.latitude, req_data.longitude) {
        common::geocode::reverse_geocode(lat, lon)
            .await
            .unwrap_or(None)
    } else {
        None
    };

    let updated_data = UpdatedListing {
        name: req_data.name,
        description: req_data.description,
        listing_structure_id: structure_id,
        country: req_data.country,
        price_per_night: req_data.price_per_night,
        is_active: req_data.is_active,
        weekly_discount_percentage: req_data.weekly_discount_percentage,
        monthly_discount_percentage: req_data.monthly_discount_percentage,
        max_guests: req_data.max_guests,
        bedrooms: req_data.bedrooms,
        beds: req_data.beds,
        full_bathrooms: req_data.full_bathrooms,
        half_bathrooms: req_data.half_bathrooms,
        square_meters: req_data.square_meters,
        latitude: req_data.latitude,
        longitude: req_data.longitude,
        listing_details: req_data.listing_details,
        city,
        base_currency: req_data.base_currency,
        minimum_stay: req_data.minimum_stay,
        days_between_bookings: req_data.days_between_bookings,
        commission_pct: req_data.commission_pct,
        cleaning_fee: req_data.cleaning_fee,
        damage_deposit: req_data.damage_deposit,
        guests_included: req_data.guests_included,
        extra_guest_fee: req_data.extra_guest_fee,
        booking_mode: req_data.booking_mode,
        location_privacy: req_data.location_privacy,
    };

    let updated_listing =
        db_listing::update_listing(pool.get_ref(), listing_id, &updated_data).await?;

    Ok(respond(
        &req,
        Payload::Item(manager_listing_response(updated_listing)),
        |_: Vec<ListingResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

/// Query parameters of listing deletion.
#[derive(Deserialize, IntoParams, Debug)]
pub struct DeleteListingParams {
    /// Delete permanently instead of soft-deleting, where enabled.
    pub hard_delete: Option<bool>,
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    delete,
    path = "/api/v1/listings/{id}",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID"),
        DeleteListingParams
    ),
    responses(
        (status = 204, description = "Listing deleted"),
        (status = 404, description = "Listing not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn delete_listing(
    claims: Claims,
    _req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    query: web::Query<DeleteListingParams>,
    settings: web::Data<Settings>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    authz::require_listing_manager(pool.get_ref(), &claims, listing_id, "delete_listing").await?;
    let hard_delete = query.hard_delete.unwrap_or(false);

    // Guard: Prevent hard delete if the feature flag is off
    if hard_delete && !settings.feature_flags.enable_hard_deletes {
        return Err(ApiError::FeatureDisabled("hard_delete".to_string()));
    }

    db_listing::delete_listing(pool.get_ref(), listing_id, hard_delete).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    post,
    path = "/api/v1/listings/{id}/images/presign",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID")
    ),
    request_body = common::models::ImagePresignRequest,
    responses(
        (status = 200, description = "Presigned URLs generated", body = [common::models::ImagePresignResponse]),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid user token"),
        (status = 403, description = "Not the listing's owner or an admin"),
        (status = 500, description = "Internal server error")
    )
)]
async fn presign_batch(
    claims: Claims,
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    img_metadata_list_json: web::Json<common::models::ImagePresignRequest>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    authz::require_listing_manager(pool.get_ref(), &claims, listing_id, "presign_images").await?;
    let img_metadata_req = img_metadata_list_json.into_inner();

    // Verify the listing exists first
    if db_listing::get_listing_by_id(pool.get_ref(), listing_id)
        .await
        .is_err()
    {
        let mut errors = validator::ValidationErrors::new();
        errors.add(
            "listing_id",
            validator::ValidationError::new("invalid listing"),
        );
        tracing::error!("Invalid listing_id: {}", listing_id);
        return Err(ApiError::ValidationError(errors));
    }

    // The upload carries this request's trace context as object metadata, so
    // the image worker's processing joins this trace.
    let traceparent =
        common::telemetry::current_trace_headers().remove(common::telemetry::TRACEPARENT);
    let upload_headers: std::collections::HashMap<String, String> = traceparent
        .iter()
        .map(|value| {
            (
                common::gcs::TRACEPARENT_METADATA_HEADER.to_owned(),
                value.clone(),
            )
        })
        .collect();

    // Generate the IDs and upload URLs first so they can be saved to the database.
    let mut db_payload = Vec::new();
    for image in &img_metadata_req.images {
        let file_id = Uuid::new_v4();
        let object_path = format!("listing_{}/image_{}", listing_id, file_id);
        let upload_url = common::gcs::generate_v4_signed_url(
            &object_path,
            &image.content_type,
            traceparent.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to generate presigned URL for listing_id: {}: {:?}",
                listing_id,
                e
            );
            ApiError::Internal
        })?;

        db_payload.push((file_id, image.clone(), upload_url));
    }

    // Add records to track images
    let inserted_images =
        match db_listing::create_listing_image_presigns(pool.get_ref(), listing_id, &db_payload)
            .await
        {
            Ok(images) => images,
            Err(e) => {
                tracing::error!("Failed to insert image presign records: {:?}", e);
                return Err(ApiError::Database(e));
            }
        };

    tracing::info!(
        "Number of image files to be uploaded: {}",
        inserted_images.len()
    );

    let mut responses = Vec::new();
    for img in inserted_images {
        responses.push(common::models::ImagePresignResponse {
            client_file_id: img.client_file_id,
            file_id: img.id,
            // Safe to unwrap here because we literally just inserted these URLs in the DB ourselves
            upload_url: img.upload_url.unwrap_or_default(),
            upload_headers: upload_headers.clone(),
        });
    }

    Ok(respond(
        &req,
        Payload::Collection(responses),
        |_: Vec<common::models::ImagePresignResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip(claims, pool))]
#[utoipa::path(
    delete,
    path = "/api/v1/listings/{id}/images/{image_id}",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID"),
        ("image_id" = String, Path, description = "Photo UUID (the original upload, not a resized variant)")
    ),
    responses(
        (status = 204, description = "Photo and its resized variants deleted"),
        (status = 404, description = "Photo not found on this listing"),
        (status = 500, description = "Internal server error")
    )
)]
async fn delete_listing_image(
    claims: Claims,
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<impl Responder, ApiError> {
    let (listing_id, image_id) = path.into_inner();
    authz::require_listing_manager(pool.get_ref(), &claims, listing_id, "delete_image").await?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Database(e.into()))?;
    db_listing::delete_listing_image(&mut tx, listing_id, image_id).await?;
    tx.commit()
        .await
        .map_err(|e| ApiError::Database(e.into()))?;
    tracing::info!(%listing_id, %image_id, "listing photo deleted");

    Ok(HttpResponse::NoContent().finish())
}

#[tracing::instrument(skip(claims, pool, body))]
#[utoipa::path(
    patch,
    path = "/api/v1/listings/{id}/images/{image_id}",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID"),
        ("image_id" = String, Path, description = "Photo UUID (the original upload, not a resized variant)")
    ),
    request_body = common::models::RenameListingImageRequest,
    responses(
        (status = 200, description = "Photo renamed; body is `{ \"id\", \"name\" }`"),
        (status = 400, description = "INVALID_PHOTO_NAME: over 80 characters"),
        (status = 404, description = "Photo not found on this listing"),
        (status = 500, description = "Internal server error")
    )
)]
async fn rename_listing_image(
    claims: Claims,
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<common::models::RenameListingImageRequest>,
) -> Result<impl Responder, ApiError> {
    let (listing_id, image_id) = path.into_inner();
    authz::require_listing_manager(pool.get_ref(), &claims, listing_id, "rename_image").await?;
    let name = common::listing_details::validate_photo_name(body.name.as_deref()).map_err(|e| {
        tracing::warn!(%listing_id, %image_id, error = %e, "photo name refused");
        let mut errors = validator::ValidationErrors::new();
        errors.add(
            "name",
            validator::ValidationError::new("INVALID_PHOTO_NAME")
                .with_message(e.to_string().into()),
        );
        ApiError::ValidationError(errors)
    })?;
    db_listing::rename_listing_image(pool.get_ref(), listing_id, image_id, name.as_deref()).await?;
    tracing::info!(%listing_id, %image_id, named = name.is_some(), "listing photo renamed");
    Ok(HttpResponse::Ok().json(serde_json::json!({ "id": image_id, "name": name })))
}

/// Registers the listing routes.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            get_listings,
            create_listing,
            get_listing_by_id,
            update_listing,
            delete_listing,
            presign_batch,
            delete_listing_image,
            rename_listing_image,
            crate::reviews::get_review_token_info,
            crate::reviews::submit_review,
            crate::reviews::submit_host_reply,
            crate::reviews::get_listing_reviews_handler,
            crate::calendar::list_feeds,
            crate::calendar::create_feed,
            crate::calendar::update_feed,
            crate::calendar::delete_feed,
            crate::calendar::sync_now,
            crate::calendar::list_blocks,
            crate::calendar::create_block,
            crate::calendar::delete_block,
            crate::calendar::list_conflicts,
            crate::calendar::acknowledge_conflict,
            crate::calendar::overview,
            crate::calendar::export_calendar,
        ),
        components(
            schemas(
                common::models::NewListingRequest,
                UpdatedListingRequest,
                ListingResponse,
                common::models::ListingDetails,
                pagination::Pagination,
                common::models::ListingFilter,
                common::models::ImagePresignRequest,
                common::models::ImagePresignResponse,
                common::models::PendingImageMetadata,
                common::models::ReviewTokenInfoResponse,
                common::models::NewReviewRequest,
                common::models::HostReplyRequest,
                common::models::ReviewResponse,
                common::calendar::dto::CalendarFeedResponse,
                common::calendar::dto::NewCalendarFeedRequest,
                common::calendar::dto::UpdateCalendarFeedRequest,
                common::calendar::dto::CreateCalendarFeedResponse,
                common::calendar::dto::UpdateCalendarFeedResponse,
                common::calendar::dto::FeedSyncResult,
                common::calendar::dto::SyncOutcome,
                common::calendar::FeedStatus,
                common::calendar::BlockSource,
                common::calendar::ConflictKind,
                common::calendar::conflict::StaleReason,
                common::calendar::dto::BlockedDateResponse,
                common::calendar::dto::NewBlockedDateRequest,
                common::calendar::dto::CalendarConflictResponse,
                common::calendar::dto::CalendarConflictsResponse,
                common::calendar::dto::CalendarOverview,
                common::calendar::dto::CalendarOverviewListing
            )
        ),
        tags(
            (name = "listings", description = "Listing management endpoints"),
            (name = "reviews", description = "Review and rating endpoints"),
            (name = "calendar", description = "iCal calendar sync: feeds, blocks, conflicts and export")
        ),
    )]
    struct ApiDoc;

    // Register Swagger UI services at the ROOT scope so paths match
    cfg.service(
        SwaggerUi::new("/api/docs/swagger-ui/{_:.*}")
            .url("/api-docs/openapi.json", ApiDoc::openapi()),
    );

    cfg.service(
        web::scope("/api/v1/listings")
            .route(
                "",
                web::get()
                    .to(get_listings)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "",
                web::post()
                    .to(create_listing)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}",
                web::get()
                    .to(get_listing_by_id)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}",
                web::patch()
                    .to(update_listing)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}",
                web::delete()
                    .to(delete_listing)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/images/presign",
                web::post()
                    .to(presign_batch)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/images/{image_id}",
                web::delete()
                    .to(delete_listing_image)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/images/{image_id}",
                web::patch()
                    .to(rename_listing_image)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/reviews",
                web::get()
                    .to(crate::reviews::get_listing_reviews_handler)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/price-overrides",
                web::get()
                    .to(get_price_overrides)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/price-overrides",
                web::post()
                    .to(create_price_override)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/price-overrides/{override_id}",
                web::put()
                    .to(update_price_override)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/price-overrides/{override_id}",
                web::delete()
                    .to(delete_price_override)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .configure(crate::calendar::configure_listing_routes),
    );

    crate::reviews::configure_routes(cfg);
    crate::calendar::configure_routes(cfg);
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    post,
    path = "/api/v1/listings/{id}/price-overrides",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID")
    ),
    request_body = common::models::CreatePriceOverrideRequest,
    responses(
        (status = 201, description = "Price override created", body = common::models::PriceOverride),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid user token"),
        (status = 403, description = "Not the listing's owner or an admin"),
        (status = 500, description = "Internal server error")
    )
)]
async fn create_price_override(
    claims: Claims,
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    req_body: web::Json<common::models::CreatePriceOverrideRequest>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    authz::require_listing_manager(pool.get_ref(), &claims, listing_id, "create_price_override")
        .await?;
    let override_req = req_body.into_inner();

    if override_req.end_date <= override_req.start_date {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "End date must be strictly after start date".to_string(),
            ),
        ));
    }
    if override_req.nightly_rate <= Decimal::ZERO {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Nightly rate must be greater than zero".to_string(),
            ),
        ));
    }
    if override_req.min_nights < 1 {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Minimum nights must be at least 1".to_string(),
            ),
        ));
    }

    let created_override =
        db_listing::create_price_override(pool.get_ref(), listing_id, &override_req)
            .await
            .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(created_override),
        |_: Vec<common::models::PriceOverride>| (),
        actix_web::http::StatusCode::CREATED,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/listings/{id}/price-overrides",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID")
    ),
    responses(
        (status = 200, description = "List of price overrides", body = Vec<common::models::PriceOverride>),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_price_overrides(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    let overrides = db_listing::get_price_overrides_by_listing(pool.get_ref(), listing_id)
        .await
        .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Collection(overrides),
        |_: Vec<common::models::PriceOverride>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    put,
    path = "/api/v1/listings/{id}/price-overrides/{override_id}",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID"),
        ("override_id" = String, Path, description = "Price Override UUID")
    ),
    request_body = common::models::UpdatePriceOverrideRequest,
    responses(
        (status = 200, description = "Price override updated", body = common::models::PriceOverride),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid user token"),
        (status = 403, description = "Not the listing's owner or an admin"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_price_override(
    claims: Claims,
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
    req_body: web::Json<common::models::UpdatePriceOverrideRequest>,
) -> Result<impl Responder, ApiError> {
    let (listing_id, override_id) = path.into_inner();
    authz::require_listing_manager(pool.get_ref(), &claims, listing_id, "update_price_override")
        .await?;
    let override_req = req_body.into_inner();

    if let (Some(start), Some(end)) = (override_req.start_date, override_req.end_date)
        && end <= start
    {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "End date must be strictly after start date".to_string(),
            ),
        ));
    }
    if let Some(rate) = override_req.nightly_rate
        && rate <= Decimal::ZERO
    {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Nightly rate must be greater than zero".to_string(),
            ),
        ));
    }
    if let Some(min_nights) = override_req.min_nights
        && min_nights < 1
    {
        return Err(ApiError::Database(
            db_core::error::DbError::ValidationError(
                "Minimum nights must be at least 1".to_string(),
            ),
        ));
    }

    let updated_override =
        db_listing::update_price_override(pool.get_ref(), override_id, listing_id, &override_req)
            .await
            .map_err(ApiError::Database)?;

    Ok(respond(
        &req,
        Payload::Item(updated_override),
        |_: Vec<common::models::PriceOverride>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument(skip(claims, pool))]
#[utoipa::path(
    delete,
    path = "/api/v1/listings/{id}/price-overrides/{override_id}",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID"),
        ("override_id" = String, Path, description = "Price Override UUID")
    ),
    responses(
        (status = 204, description = "Price override deleted"),
        (status = 404, description = "Price override not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn delete_price_override(
    claims: Claims,
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<impl Responder, ApiError> {
    let (listing_id, override_id) = path.into_inner();
    authz::require_listing_manager(pool.get_ref(), &claims, listing_id, "delete_price_override")
        .await?;
    db_listing::delete_price_override(pool.get_ref(), override_id, listing_id)
        .await
        .map_err(ApiError::Database)?;

    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
#[path = "apis_test.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "location_input_tests.rs"]
mod location_input_tests;

#[cfg(test)]
#[path = "stay_total_tests.rs"]
mod stay_total_tests;
