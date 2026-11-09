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
        .route("/{id}/calendar/sync", web::post().to(sync_now))
        .route("/{id}/calendar.ics", web::get().to(export_calendar))
        .route("/{id}/calendar/blocks", web::get().to(list_blocks))
        .route("/{id}/calendar/blocks", web::post().to(create_block))
        .route(
            "/{id}/calendar/blocks/{block_id}",
            web::delete().to(delete_block),
        )
        .route("/{id}/calendar/conflicts", web::get().to(list_conflicts))
        .route(
            "/{id}/calendar/conflicts/{conflict_id}/acknowledge",
            web::post().to(acknowledge_conflict),
        );
}

/// Registers `/api/v1/calendar/overview`.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/v1/calendar/overview", web::get().to(overview));
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

/// Query of `GET …/calendar/blocks`.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct BlocksQuery {
    /// First night shown; defaults to today (Jamaica time).
    pub from: Option<chrono::NaiveDate>,
    /// End of the range; defaults to 12 months after `from`, at most 24.
    pub to: Option<chrono::NaiveDate>,
}

/// Query of `GET …/calendar/conflicts`.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct ConflictsQuery {
    /// 1-based page of acknowledged history.
    pub page: Option<i64>,
}

fn unprocessable(message: impl Into<String>) -> ApiError {
    ApiError::Unprocessable {
        code: "VALIDATION_ERROR",
        message: message.into(),
    }
}

/// A listing's blocked dates in a range, imported and manual.
#[tracing::instrument(skip_all, fields(listing_id = %path))]
#[utoipa::path(
    get,
    path = "/api/v1/listings/{id}/calendar/blocks",
    tag = "calendar",
    params(("id" = Uuid, Path, description = "Listing id"), BlocksQuery),
    responses(
        (status = 200, description = "Blocked dates", body = [common::calendar::dto::BlockedDateResponse]),
        (status = 422, description = "VALIDATION_ERROR")
    )
)]
pub async fn list_blocks(
    claims: Claims,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    query: web::Query<BlocksQuery>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    authz::require_listing_manager(&pool, &claims, listing_id, "list_blocks").await?;
    let from = query
        .from
        .unwrap_or_else(|| common::reference::listing_today(chrono::Utc::now()));
    let to = match query.to {
        Some(to) => to,
        None => from
            .checked_add_months(chrono::Months::new(12))
            .ok_or_else(|| unprocessable("the range is out of bounds"))?,
    };
    let longest = from
        .checked_add_months(chrono::Months::new(24))
        .ok_or_else(|| unprocessable("the range is out of bounds"))?;
    if to <= from || to > longest {
        return Err(unprocessable(
            "the range must be after its start and at most 24 months",
        ));
    }
    let blocks = db_core::calendar::blocks::list_blocks(&pool, listing_id, from, to).await?;
    Ok(HttpResponse::Ok().json(blocks))
}

/// Blocks nights by hand. Refused over a stay, hold or request (409
/// `DATE_UNAVAILABLE`); an exact duplicate is 409 `CALENDAR_BLOCK_DUPLICATE`.
#[tracing::instrument(skip_all, fields(listing_id = %path))]
#[utoipa::path(
    post,
    path = "/api/v1/listings/{id}/calendar/blocks",
    tag = "calendar",
    params(("id" = Uuid, Path, description = "Listing id")),
    request_body = common::calendar::dto::NewBlockedDateRequest,
    responses(
        (status = 201, description = "Blocked", body = common::calendar::dto::BlockedDateResponse),
        (status = 409, description = "DATE_UNAVAILABLE or CALENDAR_BLOCK_DUPLICATE"),
        (status = 422, description = "VALIDATION_ERROR")
    )
)]
pub async fn create_block(
    claims: Claims,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<common::calendar::dto::NewBlockedDateRequest>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    let (user, _) =
        authz::require_listing_manager(&pool, &claims, listing_id, "create_block").await?;
    let request = body.into_inner();
    request
        .validate_for(common::reference::listing_today(chrono::Utc::now()))
        .map_err(|e| unprocessable(e.to_string()))?;
    let block =
        db_core::calendar::blocks::create_manual_block(&pool, listing_id, &request, Some(user.id))
            .await?;
    tracing::info!(block_id = %block.id, "manual block added");
    Ok(HttpResponse::Created().json(block))
}

/// Removes a manual block. An imported one is 409 `CALENDAR_BLOCK_MANAGED_BY_FEED`.
#[tracing::instrument(skip_all, fields(listing_id = %path.0, block_id = %path.1))]
#[utoipa::path(
    delete,
    path = "/api/v1/listings/{id}/calendar/blocks/{block_id}",
    tag = "calendar",
    params(
        ("id" = Uuid, Path, description = "Listing id"),
        ("block_id" = Uuid, Path, description = "Block id")
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "Block not found on this listing"),
        (status = 409, description = "CALENDAR_BLOCK_MANAGED_BY_FEED")
    )
)]
pub async fn delete_block(
    claims: Claims,
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<impl Responder, ApiError> {
    let (listing_id, block_id) = path.into_inner();
    authz::require_listing_manager(&pool, &claims, listing_id, "delete_block").await?;
    match db_core::calendar::blocks::delete_manual_block(&pool, listing_id, block_id).await? {
        true => Ok(HttpResponse::NoContent().finish()),
        false => Err(not_found()),
    }
}

/// A listing's conflicts: open ones first, then a page of history.
#[tracing::instrument(skip_all, fields(listing_id = %path))]
#[utoipa::path(
    get,
    path = "/api/v1/listings/{id}/calendar/conflicts",
    tag = "calendar",
    params(("id" = Uuid, Path, description = "Listing id"), ConflictsQuery),
    responses(
        (status = 200, description = "Conflicts", body = common::calendar::dto::CalendarConflictsResponse)
    )
)]
pub async fn list_conflicts(
    claims: Claims,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    query: web::Query<ConflictsQuery>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    authz::require_listing_manager(&pool, &claims, listing_id, "list_conflicts").await?;
    let page = query.page.unwrap_or(1).max(1);
    let conflicts = db_core::calendar::conflicts::list_conflicts(&pool, listing_id, page).await?;
    Ok(HttpResponse::Ok().json(conflicts))
}

/// Acknowledges a conflict. Repeating it is harmless.
#[tracing::instrument(skip_all, fields(listing_id = %path.0, conflict_id = %path.1))]
#[utoipa::path(
    post,
    path = "/api/v1/listings/{id}/calendar/conflicts/{conflict_id}/acknowledge",
    tag = "calendar",
    params(
        ("id" = Uuid, Path, description = "Listing id"),
        ("conflict_id" = Uuid, Path, description = "Conflict id")
    ),
    responses(
        (status = 200, description = "Acknowledged"),
        (status = 404, description = "Conflict not found on this listing")
    )
)]
pub async fn acknowledge_conflict(
    claims: Claims,
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<impl Responder, ApiError> {
    let (listing_id, conflict_id) = path.into_inner();
    let (user, _) =
        authz::require_listing_manager(&pool, &claims, listing_id, "acknowledge_conflict").await?;
    match db_core::calendar::conflicts::acknowledge(&pool, listing_id, conflict_id, user.id).await?
    {
        true => Ok(HttpResponse::Ok().json(serde_json::json!({ "acknowledged": true }))),
        false => Err(not_found()),
    }
}

/// Open conflicts and failing feeds across the caller's listings (every
/// listing for the admin).
#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/calendar/overview",
    tag = "calendar",
    responses(
        (status = 200, description = "Overview", body = common::calendar::dto::CalendarOverview),
        (status = 401, description = "Missing or invalid user token")
    )
)]
pub async fn overview(claims: Claims, pool: web::Data<PgPool>) -> Result<impl Responder, ApiError> {
    let user = authz::caller(&pool, &claims).await?;
    let overview =
        db_core::calendar::conflicts::overview_for_user(&pool, user.id, authz::is_admin(&user))
            .await?;
    Ok(HttpResponse::Ok().json(overview))
}

/// The listing's public busy-only calendar (spec §1.6). Called by the guest
/// portal with its service identity only; no user token. A strong `ETag`
/// that ignores `DTSTAMP` lets channels poll cheaply with `If-None-Match`.
#[tracing::instrument(skip_all, fields(listing_id = %path))]
#[utoipa::path(
    get,
    path = "/api/v1/listings/{id}/calendar.ics",
    tag = "calendar",
    params(("id" = Uuid, Path, description = "Listing id")),
    responses(
        (status = 200, description = "RFC 5545 calendar", content_type = "text/calendar"),
        (status = 304, description = "Unchanged since the given ETag"),
        (status = 404, description = "Unknown or deleted listing")
    )
)]
pub async fn export_calendar(
    req: actix_web::HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let listing_id = path.into_inner();
    let now = chrono::Utc::now();
    let today = common::reference::listing_today(now);
    let Some((name, events)) =
        db_core::calendar::export::export_events(&pool, listing_id, today).await?
    else {
        return Ok(HttpResponse::NotFound()
            .content_type("text/plain; charset=utf-8")
            .body("Not found"));
    };
    let body = common::calendar::ics_write::write_feed(&name, &events, now);
    let etag = common::calendar::ics_write::etag(&body);
    let matches = req
        .headers()
        .get(actix_web::http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',')
                .any(|tag| tag.trim() == etag || tag.trim() == "*")
        });
    let mut resp = match matches {
        true => HttpResponse::NotModified(),
        false => HttpResponse::Ok(),
    };
    resp.insert_header((actix_web::http::header::ETAG, etag))
        .insert_header((
            actix_web::http::header::CACHE_CONTROL,
            "public, max-age=300",
        ));
    if matches {
        return Ok(resp.finish());
    }
    Ok(resp
        .content_type("text/calendar; charset=utf-8")
        .insert_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{listing_id}.ics\""),
        ))
        .body(body))
}
