//! Calendar sync routes (spec §3.5): imported feeds, "Sync now", blocked
//! dates, conflicts, the dashboard overview and the export feed.
//!
//! Every route under `/api/v1/listings/{id}/calendar` requires a listing
//! manager ([`crate::authz::require_listing_manager`]). Child ids are always
//! matched together with the listing id. Feed URLs are secrets: they're only
//! returned to managers and never logged.

use crate::authz;
use actix_web::{HttpResponse, Responder, web};
use api_core::auth::Claims;
use api_core::calendar::sync::{OutboxPublisher, SyncDeps, sync_feeds};
use api_core::calendar::{GuardedFetcher, fetcher_config};
use api_core::error::ApiError;
use api_core::settings::CalendarSyncSettings;
use chrono::Duration;
use common::calendar::dto::{
    CalendarFeedResponse, CreateCalendarFeedResponse, FeedSyncResult, NewCalendarFeedRequest,
    SyncOutcome, UpdateCalendarFeedRequest, UpdateCalendarFeedResponse, clean_feed_name,
};
use common::calendar::feed_url::{FeedUrlError, validate_feed_url};
use db_core::calendar::feeds::{self, FeedChange, NewFeed};
use db_core::calendar::reconcile::{SyncContext, SyncTrigger};
use db_core::error::DbError;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::OnceCell;
use uuid::Uuid;

/// How often one listing's feeds may be synced on demand.
pub const MANUAL_SYNC_INTERVAL_SECS: i64 = 60;
/// Time a request may spend fetching and parsing before feeds are deferred.
pub const HANDLER_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);
/// Feeds fetched at once for one request.
pub const HANDLER_CONCURRENCY: usize = 5;
/// Env var: the admin portal's base URL, for links in emails.
pub const ADMIN_BASE_URL_ENV: &str = "ADMIN_BASE_URL";

/// What the calendar routes share: the sync settings, the lazily built
/// fetcher, the email publisher and the admin portal URL.
pub struct CalendarRuntime {
    settings: CalendarSyncSettings,
    fetcher: OnceCell<GuardedFetcher>,
    publisher: Arc<dyn OutboxPublisher>,
    admin_base_url: String,
    deadline: std::time::Duration,
}

impl CalendarRuntime {
    /// Replaces the handler deadline (tests).
    pub fn with_deadline(mut self, deadline: std::time::Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// A runtime that builds its fetcher from `settings` on first use, so
    /// startup does no extra work.
    pub fn new(
        settings: CalendarSyncSettings,
        publisher: Arc<dyn OutboxPublisher>,
        admin_base_url: String,
    ) -> Self {
        CalendarRuntime {
            settings,
            fetcher: OnceCell::new(),
            publisher,
            admin_base_url,
            deadline: HANDLER_DEADLINE,
        }
    }

    /// A runtime with a ready fetcher (tests).
    pub fn with_fetcher(
        fetcher: GuardedFetcher,
        publisher: Arc<dyn OutboxPublisher>,
        admin_base_url: String,
    ) -> Self {
        CalendarRuntime {
            settings: CalendarSyncSettings::default(),
            fetcher: OnceCell::new_with(Some(fetcher)),
            publisher,
            admin_base_url,
            deadline: HANDLER_DEADLINE,
        }
    }

    async fn fetcher(&self) -> Result<GuardedFetcher, ApiError> {
        self.fetcher
            .get_or_try_init(|| async {
                let config = fetcher_config(&self.settings).await?;
                GuardedFetcher::new(config)
            })
            .await
            .cloned()
            .map_err(|e| {
                tracing::error!(code = e.code(), "calendar fetcher unavailable");
                ApiError::Internal
            })
    }

    fn context(&self) -> SyncContext {
        SyncContext {
            trigger: SyncTrigger::Manual,
            admin_base_url: self.admin_base_url.clone(),
        }
    }
}

/// Registers the routes under `/api/v1/listings` (call from that scope).
pub fn configure_listing_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/{id}/calendar/feeds", web::get().to(list_feeds))
        .route("/{id}/calendar/feeds", web::post().to(create_feed))
        .route(
            "/{id}/calendar/feeds/{feed_id}",
            web::patch().to(update_feed),
        )
        .route(
            "/{id}/calendar/feeds/{feed_id}",
            web::delete().to(delete_feed),
        )
        .route("/{id}/calendar/sync", web::post().to(sync_now));
}

fn invalid_url(e: FeedUrlError) -> ApiError {
    ApiError::Unprocessable {
        code: e.code(),
        message: e.to_string(),
    }
}

fn invalid_name(e: common::calendar::dto::FeedNameError) -> ApiError {
    ApiError::Unprocessable {
        code: "VALIDATION_ERROR",
        message: e.to_string(),
    }
}

fn not_found() -> ApiError {
    ApiError::Database(DbError::Sqlx(sqlx::Error::RowNotFound))
}

fn rate_limited(retry_after_secs: i64) -> ApiError {
    ApiError::Database(DbError::CalendarSyncRateLimited {
        retry_after_secs: u64::try_from(retry_after_secs).unwrap_or(1),
    })
}

/// Refuses an on-demand sync of `listing_id` within the interval.
async fn check_rate(pool: &PgPool, listing_id: Uuid) -> Result<(), ApiError> {
    match feeds::listing_retry_after(
        pool,
        listing_id,
        Duration::seconds(MANUAL_SYNC_INTERVAL_SECS),
    )
    .await?
    {
        Some(wait) => Err(rate_limited(wait)),
        None => Ok(()),
    }
}

/// Claims the listing's due feeds and syncs them within the handler deadline.
/// Feeds synced too recently come back `skipped`.
async fn sync_listing(
    pool: &PgPool,
    runtime: &CalendarRuntime,
    listing_id: Uuid,
) -> Result<(Vec<FeedSyncResult>, Option<i64>), ApiError> {
    let claim = feeds::claim_for_listing(
        pool,
        listing_id,
        Duration::seconds(MANUAL_SYNC_INTERVAL_SECS),
    )
    .await?;
    let mut results = match claim.claims.is_empty() {
        true => Vec::new(),
        false => {
            let deps = SyncDeps {
                pool: pool.clone(),
                publisher: runtime.publisher.clone(),
                fetcher: runtime.fetcher().await?,
            };
            let deadline = tokio::time::Instant::now() + runtime.deadline;
            sync_feeds(
                &deps,
                claim.claims,
                &runtime.context(),
                HANDLER_CONCURRENCY,
                deadline,
            )
            .await
        }
    };
    results.extend(claim.skipped.into_iter().map(|feed_id| FeedSyncResult {
        feed_id,
        outcome: SyncOutcome::Skipped,
        events: None,
        error_code: None,
        conflicts_opened: 0,
    }));
    Ok((results, claim.retry_after_secs))
}

/// The result for `feed_id` among `results`, or `skipped`.
fn result_for(results: &[FeedSyncResult], feed_id: Uuid) -> FeedSyncResult {
    results
        .iter()
        .find(|r| r.feed_id == feed_id)
        .cloned()
        .unwrap_or(FeedSyncResult {
            feed_id,
            outcome: SyncOutcome::Skipped,
            events: None,
            error_code: None,
            conflicts_opened: 0,
        })
}

async fn reload(
    pool: &PgPool,
    listing_id: Uuid,
    feed_id: Uuid,
) -> Result<CalendarFeedResponse, ApiError> {
    feeds::get(pool, listing_id, feed_id)
        .await?
        .map(Into::into)
        .ok_or_else(not_found)
}

/// Lists a listing's imported calendars.
#[tracing::instrument(skip_all, fields(listing_id = %path))]
#[utoipa::path(
    get,
    path = "/api/v1/listings/{id}/calendar/feeds",
    tag = "calendar",
    params(("id" = Uuid, Path, description = "Listing id")),
    responses(
        (status = 200, description = "Imported calendars", body = [CalendarFeedResponse]),
        (status = 401, description = "Missing or invalid user token"),
        (status = 403, description = "Not the listing's owner or an admin"),
        (status = 404, description = "Listing not found")
    )
)]
pub async fn list_feeds(
    claims: Claims,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    authz::require_listing_manager(&pool, &claims, listing_id, "list_calendar_feeds").await?;
    let rows: Vec<CalendarFeedResponse> = feeds::list(&pool, listing_id)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(HttpResponse::Ok().json(rows))
}

/// Adds an imported calendar and runs its first sync. A failed first sync
/// still saves the feed (201, `last_status = failed`).
#[tracing::instrument(skip_all, fields(listing_id = %path))]
#[utoipa::path(
    post,
    path = "/api/v1/listings/{id}/calendar/feeds",
    tag = "calendar",
    params(("id" = Uuid, Path, description = "Listing id")),
    request_body = NewCalendarFeedRequest,
    responses(
        (status = 201, description = "Saved, with its first sync's result", body = CreateCalendarFeedResponse),
        (status = 401, description = "Missing or invalid user token"),
        (status = 403, description = "Not the listing's owner or an admin"),
        (status = 409, description = "CALENDAR_FEED_LIMIT or CALENDAR_FEED_DUPLICATE"),
        (status = 422, description = "CALENDAR_FEED_INVALID_URL, CALENDAR_FEED_SELF or VALIDATION_ERROR"),
        (status = 429, description = "CALENDAR_SYNC_RATE_LIMITED, with Retry-After")
    )
)]
pub async fn create_feed(
    claims: Claims,
    pool: web::Data<PgPool>,
    runtime: web::Data<CalendarRuntime>,
    path: web::Path<Uuid>,
    body: web::Json<NewCalendarFeedRequest>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    let (user, _) =
        authz::require_listing_manager(&pool, &claims, listing_id, "create_calendar_feed").await?;
    let name = clean_feed_name(&body.name).map_err(invalid_name)?;
    let url = validate_feed_url(&body.url, listing_id).map_err(invalid_url)?;
    check_rate(&pool, listing_id).await?;
    let feed = feeds::create(
        &pool,
        &NewFeed {
            listing_id,
            name: &name,
            url: url.as_str(),
            created_by: Some(user.id),
        },
    )
    .await?;
    tracing::info!(feed_id = %feed.id, host = %common::calendar::feed_url::redact(&url), "calendar feed added");
    let (results, _) = sync_listing(&pool, &runtime, listing_id).await?;
    let sync = result_for(&results, feed.id);
    let feed = reload(&pool, listing_id, feed.id).await?;
    Ok(HttpResponse::Created().json(CreateCalendarFeedResponse { feed, sync }))
}

/// Renames, re-points or switches an imported calendar. A new URL is checked
/// again and synced; the old blocks stay until it syncs.
#[tracing::instrument(skip_all, fields(listing_id = %path.0, feed_id = %path.1))]
#[utoipa::path(
    patch,
    path = "/api/v1/listings/{id}/calendar/feeds/{feed_id}",
    tag = "calendar",
    params(
        ("id" = Uuid, Path, description = "Listing id"),
        ("feed_id" = Uuid, Path, description = "Feed id")
    ),
    request_body = UpdateCalendarFeedRequest,
    responses(
        (status = 200, description = "Updated", body = UpdateCalendarFeedResponse),
        (status = 404, description = "Feed not found on this listing"),
        (status = 409, description = "CALENDAR_FEED_DUPLICATE"),
        (status = 422, description = "Invalid name or URL"),
        (status = 429, description = "CALENDAR_SYNC_RATE_LIMITED, with Retry-After")
    )
)]
pub async fn update_feed(
    claims: Claims,
    pool: web::Data<PgPool>,
    runtime: web::Data<CalendarRuntime>,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<UpdateCalendarFeedRequest>,
) -> Result<impl Responder, ApiError> {
    let (listing_id, feed_id) = path.into_inner();
    authz::require_listing_manager(&pool, &claims, listing_id, "update_calendar_feed").await?;
    let name = body
        .name
        .as_deref()
        .map(clean_feed_name)
        .transpose()
        .map_err(invalid_name)?;
    let url = body
        .url
        .as_deref()
        .map(|raw| validate_feed_url(raw, listing_id))
        .transpose()
        .map_err(invalid_url)?;
    let current = feeds::get(&pool, listing_id, feed_id)
        .await?
        .ok_or_else(not_found)?;
    let url = url.filter(|u| u.as_str() != current.url);
    if url.is_some() {
        check_rate(&pool, listing_id).await?;
    }
    let updated = feeds::update(
        &pool,
        listing_id,
        feed_id,
        &FeedChange {
            name: name.as_deref(),
            url: url.as_ref().map(url::Url::as_str),
            is_active: body.is_active,
        },
    )
    .await?
    .ok_or_else(not_found)?;
    let sync = match updated.url_changed && updated.feed.is_active {
        true => {
            let (results, _) = sync_listing(&pool, &runtime, listing_id).await?;
            Some(result_for(&results, feed_id))
        }
        false => None,
    };
    let feed = reload(&pool, listing_id, feed_id).await?;
    Ok(HttpResponse::Ok().json(UpdateCalendarFeedResponse { feed, sync }))
}

/// Removes an imported calendar and its blocks; its conflicts go stale.
#[tracing::instrument(skip_all, fields(listing_id = %path.0, feed_id = %path.1))]
#[utoipa::path(
    delete,
    path = "/api/v1/listings/{id}/calendar/feeds/{feed_id}",
    tag = "calendar",
    params(
        ("id" = Uuid, Path, description = "Listing id"),
        ("feed_id" = Uuid, Path, description = "Feed id")
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "Feed not found on this listing")
    )
)]
pub async fn delete_feed(
    claims: Claims,
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<impl Responder, ApiError> {
    let (listing_id, feed_id) = path.into_inner();
    authz::require_listing_manager(&pool, &claims, listing_id, "delete_calendar_feed").await?;
    match feeds::delete(&pool, listing_id, feed_id).await? {
        true => {
            tracing::info!("calendar feed removed");
            Ok(HttpResponse::NoContent().finish())
        }
        false => Err(not_found()),
    }
}

/// Syncs the listing's imported calendars now, within 20 s. Feeds synced in
/// the last minute come back `skipped`; if none could be synced, 429.
#[tracing::instrument(skip_all, fields(listing_id = %path))]
#[utoipa::path(
    post,
    path = "/api/v1/listings/{id}/calendar/sync",
    tag = "calendar",
    params(("id" = Uuid, Path, description = "Listing id")),
    responses(
        (status = 200, description = "Each feed's result (empty with no active feeds)", body = [FeedSyncResult]),
        (status = 429, description = "CALENDAR_SYNC_RATE_LIMITED, with Retry-After")
    )
)]
pub async fn sync_now(
    claims: Claims,
    pool: web::Data<PgPool>,
    runtime: web::Data<CalendarRuntime>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    authz::require_listing_manager(&pool, &claims, listing_id, "sync_calendar").await?;
    let (results, retry_after) = sync_listing(&pool, &runtime, listing_id).await?;
    let synced = results.iter().any(|r| r.outcome != SyncOutcome::Skipped);
    match (synced, results.is_empty(), retry_after) {
        (false, false, Some(wait)) => Err(rate_limited(wait)),
        _ => Ok(HttpResponse::Ok().json(results)),
    }
}
