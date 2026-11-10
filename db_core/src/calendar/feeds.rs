//! Imported calendar feeds: storage, edits and atomic claiming (spec §1.1,
//! §1.4.4).
//!
//! Lock order is listing → feed for every edit. Claiming never holds a lock
//! across the HTTP fetch: it stamps `last_attempt_at` and returns.
//!
//! Feed URLs carry a read token. The row types redact them in `Debug`, and a
//! unique violation (whose detail contains the URL) is mapped before anything
//! can log it.

use crate::error::{DbError, Result, SQLSTATE_UNIQUE_VIOLATION, sqlstate};
use chrono::{DateTime, Duration, Utc};
use common::calendar::FeedStatus;
use common::calendar::dto::{CalendarFeedResponse, MAX_FEEDS_PER_LISTING};
use sqlx::{PgConnection, PgPool};
use std::fmt;
use uuid::Uuid;

/// A `listing_calendar_feed` row.
#[derive(Clone, PartialEq)]
pub struct FeedRow {
    /// Feed id.
    pub id: Uuid,
    /// Listing the feed blocks dates on.
    pub listing_id: Uuid,
    /// Display name.
    pub name: String,
    /// Feed URL. Secret.
    pub url: String,
    /// Whether the feed is synced.
    pub is_active: bool,
    /// Bumped on every URL change, so stale syncs are thrown away.
    pub sync_generation: i32,
    /// Outcome of the last sync.
    pub last_status: FeedStatus,
    /// Sanitised reason of the last failure.
    pub last_error: Option<String>,
    /// Warning from the last success.
    pub last_warning: Option<String>,
    /// When a sync last started.
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// When a sync last succeeded.
    pub last_success_at: Option<DateTime<Utc>>,
    /// Scheduled syncs that failed in a row.
    pub consecutive_failures: i32,
    /// When the host was told about the failures.
    pub failure_alerted_at: Option<DateTime<Utc>>,
    /// Events imported by the last success.
    pub event_count: i32,
    /// Who added the feed.
    pub created_by: Option<Uuid>,
    /// When it was added.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

fn url_host(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| common::calendar::feed_url::redact(&u))
        .unwrap_or_else(|_| "<invalid>".to_string())
}

impl fmt::Debug for FeedRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FeedRow")
            .field("id", &self.id)
            .field("listing_id", &self.listing_id)
            .field("name", &self.name)
            .field("url_host", &url_host(&self.url))
            .field("is_active", &self.is_active)
            .field("sync_generation", &self.sync_generation)
            .field("last_status", &self.last_status)
            .field("last_error", &self.last_error)
            .field("consecutive_failures", &self.consecutive_failures)
            .finish_non_exhaustive()
    }
}

impl From<FeedRow> for CalendarFeedResponse {
    fn from(row: FeedRow) -> Self {
        CalendarFeedResponse {
            id: row.id,
            listing_id: row.listing_id,
            name: row.name,
            url: row.url,
            is_active: row.is_active,
            last_status: row.last_status,
            last_error: row.last_error,
            last_warning: row.last_warning,
            last_attempt_at: row.last_attempt_at,
            last_success_at: row.last_success_at,
            event_count: row.event_count,
            consecutive_failures: row.consecutive_failures,
            created_at: row.created_at,
        }
    }
}

/// A feed claimed for one sync attempt.
#[derive(Clone, PartialEq, Eq)]
pub struct FeedClaim {
    /// Feed id.
    pub id: Uuid,
    /// Its listing.
    pub listing_id: Uuid,
    /// URL to fetch. Secret.
    pub url: String,
    /// Generation at claim time; a mismatch later throws the result away.
    pub sync_generation: i32,
    /// `last_attempt_at` before this claim, restored by [`release_claim`].
    pub previous_attempt_at: Option<DateTime<Utc>>,
    /// `last_attempt_at` this claim wrote.
    pub claimed_at: DateTime<Utc>,
}

impl fmt::Debug for FeedClaim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FeedClaim")
            .field("id", &self.id)
            .field("listing_id", &self.listing_id)
            .field("url_host", &url_host(&self.url))
            .field("sync_generation", &self.sync_generation)
            .finish_non_exhaustive()
    }
}

/// A new feed. `url` is already validated and normalised.
pub struct NewFeed<'a> {
    /// Listing to add it to.
    pub listing_id: Uuid,
    /// Cleaned display name.
    pub name: &'a str,
    /// Normalised URL.
    pub url: &'a str,
    /// Who adds it.
    pub created_by: Option<Uuid>,
}

/// Changes to a feed. `None` leaves a field as it is.
#[derive(Default)]
pub struct FeedChange<'a> {
    /// New cleaned name.
    pub name: Option<&'a str>,
    /// New validated, normalised URL.
    pub url: Option<&'a str>,
    /// Switch syncing on or off.
    pub is_active: Option<bool>,
}

/// A feed after [`update`].
#[derive(Debug, Clone, PartialEq)]
pub struct UpdatedFeed {
    /// The feed as stored.
    pub feed: FeedRow,
    /// Whether the URL changed, so a sync should follow.
    pub url_changed: bool,
}

/// The result of claiming one listing's feeds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListingClaim {
    /// Feeds claimed.
    pub claims: Vec<FeedClaim>,
    /// Active feeds not claimed, because they synced too recently.
    pub skipped: Vec<Uuid>,
    /// Seconds until the earliest skipped feed can be claimed.
    pub retry_after_secs: Option<i64>,
}

/// Maps a unique violation on the feed table to [`DbError::CalendarFeedDuplicate`].
/// The violation's detail contains the URL, so this runs before any logging.
pub(crate) fn map_unique(e: sqlx::Error) -> DbError {
    match sqlstate(&e).as_deref() {
        Some(SQLSTATE_UNIQUE_VIOLATION) => DbError::CalendarFeedDuplicate,
        _ => DbError::Sqlx(e),
    }
}

/// Locks a live listing. A missing or deleted listing is `RowNotFound`.
pub(crate) async fn lock_listing(conn: &mut PgConnection, listing_id: Uuid) -> Result<()> {
    sqlx::query_scalar!(
        "SELECT id FROM listing WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
        listing_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .map(|_| ())
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))
}

/// Adds a feed, checking the 5-feed limit and duplicates under the listing lock.
#[tracing::instrument(skip_all, fields(listing_id = %feed.listing_id))]
pub async fn create(pool: &PgPool, feed: &NewFeed<'_>) -> Result<FeedRow> {
    let mut tx = pool.begin().await?;
    lock_listing(&mut tx, feed.listing_id).await?;
    let existing = sqlx::query!(
        r#"SELECT COUNT(*) AS "count!", COALESCE(bool_or(url = $2), FALSE) AS "duplicate!"
        FROM listing_calendar_feed WHERE listing_id = $1"#,
        feed.listing_id,
        feed.url
    )
    .fetch_one(&mut *tx)
    .await?;
    if existing.duplicate {
        return Err(DbError::CalendarFeedDuplicate);
    }
    if existing.count >= MAX_FEEDS_PER_LISTING {
        return Err(DbError::CalendarFeedLimit);
    }
    let row = sqlx::query_as!(
        FeedRow,
        r#"INSERT INTO listing_calendar_feed (listing_id, name, url, created_by)
        VALUES ($1, $2, $3, $4)
        RETURNING id, listing_id, name, url, is_active, sync_generation,
            last_status AS "last_status: FeedStatus", last_error, last_warning,
            last_attempt_at, last_success_at, consecutive_failures, failure_alerted_at,
            event_count, created_by, created_at, updated_at"#,
        feed.listing_id,
        feed.name,
        feed.url,
        feed.created_by
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(map_unique)?;
    tx.commit().await?;
    Ok(row)
}

/// A listing's feeds, oldest first.
#[tracing::instrument(skip(pool))]
pub async fn list(pool: &PgPool, listing_id: Uuid) -> Result<Vec<FeedRow>> {
    Ok(sqlx::query_as!(
        FeedRow,
        r#"SELECT id, listing_id, name, url, is_active, sync_generation,
            last_status AS "last_status: FeedStatus", last_error, last_warning,
            last_attempt_at, last_success_at, consecutive_failures, failure_alerted_at,
            event_count, created_by, created_at, updated_at
        FROM listing_calendar_feed WHERE listing_id = $1
        ORDER BY created_at, id"#,
        listing_id
    )
    .fetch_all(pool)
    .await?)
}

/// One feed of a listing; `None` if it's not that listing's.
#[tracing::instrument(skip(pool))]
pub async fn get(pool: &PgPool, listing_id: Uuid, feed_id: Uuid) -> Result<Option<FeedRow>> {
    Ok(sqlx::query_as!(
        FeedRow,
        r#"SELECT id, listing_id, name, url, is_active, sync_generation,
            last_status AS "last_status: FeedStatus", last_error, last_warning,
            last_attempt_at, last_success_at, consecutive_failures, failure_alerted_at,
            event_count, created_by, created_at, updated_at
        FROM listing_calendar_feed WHERE id = $1 AND listing_id = $2"#,
        feed_id,
        listing_id
    )
    .fetch_optional(pool)
    .await?)
}

/// Renames, re-points or switches a feed. A new URL bumps the generation and
/// resets the status, so the new URL can be claimed straight away; the old
/// blocks stay until it syncs. `None` if the feed isn't the listing's.
#[tracing::instrument(skip_all, fields(listing_id = %listing_id, feed_id = %feed_id))]
pub async fn update(
    pool: &PgPool,
    listing_id: Uuid,
    feed_id: Uuid,
    change: &FeedChange<'_>,
) -> Result<Option<UpdatedFeed>> {
    let mut tx = pool.begin().await?;
    lock_listing(&mut tx, listing_id).await?;
    let Some(current) = sqlx::query_scalar!(
        "SELECT url FROM listing_calendar_feed WHERE id = $1 AND listing_id = $2 FOR UPDATE",
        feed_id,
        listing_id
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(None);
    };
    let new_url = change.url.filter(|url| *url != current);
    if let Some(url) = new_url {
        let taken = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM listing_calendar_feed
                WHERE listing_id = $1 AND url = $2 AND id <> $3) AS "taken!""#,
            listing_id,
            url,
            feed_id
        )
        .fetch_one(&mut *tx)
        .await?;
        if taken {
            return Err(DbError::CalendarFeedDuplicate);
        }
    }
    let url_changed = new_url.is_some();
    let feed = sqlx::query_as!(
        FeedRow,
        r#"UPDATE listing_calendar_feed SET
            name = COALESCE($3, name),
            is_active = COALESCE($4, is_active),
            url = COALESCE($5, url),
            sync_generation = sync_generation + CASE WHEN $6 THEN 1 ELSE 0 END,
            last_attempt_at = CASE WHEN $6 THEN NULL ELSE last_attempt_at END,
            last_status = CASE WHEN $6 THEN 'never'::calendar_feed_status ELSE last_status END,
            last_error = CASE WHEN $6 THEN NULL ELSE last_error END,
            last_warning = CASE WHEN $6 THEN NULL ELSE last_warning END,
            consecutive_failures = CASE WHEN $6 THEN 0 ELSE consecutive_failures END,
            failure_alerted_at = CASE WHEN $6 THEN NULL ELSE failure_alerted_at END,
            updated_at = NOW()
        WHERE id = $1 AND listing_id = $2
        RETURNING id, listing_id, name, url, is_active, sync_generation,
            last_status AS "last_status: FeedStatus", last_error, last_warning,
            last_attempt_at, last_success_at, consecutive_failures, failure_alerted_at,
            event_count, created_by, created_at, updated_at"#,
        feed_id,
        listing_id,
        change.name,
        change.is_active,
        new_url,
        url_changed
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(map_unique)?;
    tx.commit().await?;
    Ok(Some(UpdatedFeed { feed, url_changed }))
}

/// Deletes a feed and its blocks; its conflicts stay, marked removed.
/// `false` if the feed isn't the listing's.
#[tracing::instrument(skip(pool))]
pub async fn delete(pool: &PgPool, listing_id: Uuid, feed_id: Uuid) -> Result<bool> {
    let mut tx = pool.begin().await?;
    lock_listing(&mut tx, listing_id).await?;
    let locked = sqlx::query_scalar!(
        "SELECT id FROM listing_calendar_feed WHERE id = $1 AND listing_id = $2 FOR UPDATE",
        feed_id,
        listing_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    if locked.is_none() {
        return Ok(false);
    }
    sqlx::query!(
        "DELETE FROM listing_calendar_feed WHERE id = $1 AND listing_id = $2",
        feed_id,
        listing_id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// Claims up to `limit` active feeds of live listings not attempted within
/// `min_interval`, oldest attempt first. Concurrent claimers never get the
/// same feed (`SKIP LOCKED`).
#[tracing::instrument(skip(pool))]
pub async fn claim_due(
    pool: &PgPool,
    min_interval: Duration,
    limit: i64,
) -> Result<Vec<FeedClaim>> {
    Ok(sqlx::query_as!(
        FeedClaim,
        r#"WITH due AS (
            SELECT f.id, f.last_attempt_at AS previous
            FROM listing_calendar_feed f JOIN listing l ON l.id = f.listing_id
            WHERE f.is_active AND l.deleted_at IS NULL
              AND (f.last_attempt_at IS NULL
                   OR f.last_attempt_at < now() - ($1::int8 * INTERVAL '1 second'))
            ORDER BY f.last_attempt_at NULLS FIRST, f.id
            LIMIT $2
            FOR UPDATE OF f SKIP LOCKED
        )
        UPDATE listing_calendar_feed f SET last_attempt_at = now()
        FROM due WHERE f.id = due.id
        RETURNING f.id, f.listing_id, f.url, f.sync_generation,
            due.previous AS previous_attempt_at, f.last_attempt_at AS "claimed_at!""#,
        min_interval.num_seconds(),
        limit
    )
    .fetch_all(pool)
    .await?)
}

/// Claims one listing's active feeds not attempted within `min_interval`,
/// and reports the ones skipped and when the earliest can be claimed.
#[tracing::instrument(skip(pool))]
pub async fn claim_for_listing(
    pool: &PgPool,
    listing_id: Uuid,
    min_interval: Duration,
) -> Result<ListingClaim> {
    let mut claims = sqlx::query_as!(
        FeedClaim,
        r#"WITH due AS (
            SELECT f.id, f.last_attempt_at AS previous
            FROM listing_calendar_feed f JOIN listing l ON l.id = f.listing_id
            WHERE f.is_active AND l.deleted_at IS NULL AND f.listing_id = $1
              AND (f.last_attempt_at IS NULL
                   OR f.last_attempt_at < now() - ($2::int8 * INTERVAL '1 second'))
            ORDER BY f.last_attempt_at NULLS FIRST, f.id
            FOR UPDATE OF f SKIP LOCKED
        )
        UPDATE listing_calendar_feed f SET last_attempt_at = now()
        FROM due WHERE f.id = due.id
        RETURNING f.id, f.listing_id, f.url, f.sync_generation,
            due.previous AS previous_attempt_at, f.last_attempt_at AS "claimed_at!""#,
        listing_id,
        min_interval.num_seconds()
    )
    .fetch_all(pool)
    .await?;
    claims.sort_by_key(|c| c.id);
    let claimed: Vec<Uuid> = claims.iter().map(|c| c.id).collect();
    let rows = sqlx::query!(
        r#"SELECT f.id,
            GREATEST(1, CEIL(EXTRACT(EPOCH FROM
                (COALESCE(f.last_attempt_at, now()) + ($3::int8 * INTERVAL '1 second') - now()))))::int8
                AS "wait!"
        FROM listing_calendar_feed f JOIN listing l ON l.id = f.listing_id
        WHERE f.listing_id = $1 AND f.is_active AND l.deleted_at IS NULL
          AND NOT (f.id = ANY($2::uuid[]))
        ORDER BY f.id"#,
        listing_id,
        &claimed,
        min_interval.num_seconds()
    )
    .fetch_all(pool)
    .await?;
    Ok(ListingClaim {
        claims,
        retry_after_secs: rows.iter().map(|r| r.wait).min(),
        skipped: rows.into_iter().map(|r| r.id).collect(),
    })
}

/// Gives back a claim that never started, restoring the previous attempt
/// time. Does nothing if the feed was claimed again since.
#[tracing::instrument(skip(pool, claim), fields(feed_id = %claim.id))]
pub async fn release_claim(pool: &PgPool, claim: &FeedClaim) -> Result<bool> {
    let released = sqlx::query!(
        "UPDATE listing_calendar_feed SET last_attempt_at = $2
         WHERE id = $1 AND last_attempt_at = $3",
        claim.id,
        claim.previous_attempt_at,
        claim.claimed_at
    )
    .execute(pool)
    .await?;
    Ok(released.rows_affected() == 1)
}

/// Seconds until the listing's feeds may sync again, if any active feed was
/// attempted within `min_interval`. Feed creation and URL changes use this,
/// so they can't be used to fetch more often than "Sync now".
#[tracing::instrument(skip(pool))]
pub async fn listing_retry_after(
    pool: &PgPool,
    listing_id: Uuid,
    min_interval: Duration,
) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT GREATEST(1, CEIL(EXTRACT(EPOCH FROM
                (MAX(last_attempt_at) + ($2::int8 * INTERVAL '1 second') - now()))))::int8
        FROM listing_calendar_feed
        WHERE listing_id = $1 AND is_active
        HAVING MAX(last_attempt_at) > now() - ($2::int8 * INTERVAL '1 second')"#,
        listing_id,
        min_interval.num_seconds()
    )
    .fetch_optional(pool)
    .await?
    .flatten())
}
