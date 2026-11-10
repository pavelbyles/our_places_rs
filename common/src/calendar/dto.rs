//! Request and response types for the calendar sync API (spec §3.2, §3.5).
//!
//! Feed URLs carry a read token, so every type holding one redacts it in
//! `Debug`.

use super::conflict::StaleReason;
use super::feed_url::redact;
use super::ics_parse::FEED_WINDOW_MONTHS;
use super::types::{BlockSource, ConflictKind, FeedStatus};
use chrono::{DateTime, Months, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

/// Most feeds a listing can import.
pub const MAX_FEEDS_PER_LISTING: i64 = 5;
/// Longest feed name, in characters.
pub const MAX_FEED_NAME_CHARS: usize = 60;
/// Longest manual block note, in characters.
pub const MAX_BLOCK_NOTE_CHARS: usize = 500;
/// Longest manual block, in nights.
pub const MAX_BLOCK_NIGHTS: i64 = 365;
/// Acknowledged conflicts per history page.
pub const CONFLICT_PAGE_SIZE: i64 = 20;

fn redacted_url(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| redact(&u))
        .unwrap_or_else(|_| "<invalid>".to_string())
}

/// An imported calendar, as its managers see it.
#[derive(Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CalendarFeedResponse {
    /// Feed id.
    pub id: Uuid,
    /// Listing the feed blocks dates on.
    pub listing_id: Uuid,
    /// Display name, such as "Airbnb".
    pub name: String,
    /// The feed URL. Secret: only listing managers receive it.
    pub url: String,
    /// Whether the feed is synced.
    pub is_active: bool,
    /// Outcome of the last sync.
    pub last_status: FeedStatus,
    /// Sanitised error code of the last failure.
    pub last_error: Option<String>,
    /// Warning from the last successful sync.
    pub last_warning: Option<String>,
    /// When a sync last started.
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// When a sync last succeeded.
    pub last_success_at: Option<DateTime<Utc>>,
    /// Events imported by the last successful sync.
    pub event_count: i32,
    /// Scheduled syncs that failed in a row.
    pub consecutive_failures: i32,
    /// When the feed was added.
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for CalendarFeedResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CalendarFeedResponse")
            .field("id", &self.id)
            .field("listing_id", &self.listing_id)
            .field("name", &self.name)
            .field("url_host", &redacted_url(&self.url))
            .field("is_active", &self.is_active)
            .field("last_status", &self.last_status)
            .field("last_error", &self.last_error)
            .field("event_count", &self.event_count)
            .field("consecutive_failures", &self.consecutive_failures)
            .finish_non_exhaustive()
    }
}

/// Body of `POST …/calendar/feeds`.
#[derive(Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct NewCalendarFeedRequest {
    /// Display name, 1–60 characters after trimming.
    pub name: String,
    /// The channel's export URL (`https` only).
    pub url: String,
}

impl fmt::Debug for NewCalendarFeedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewCalendarFeedRequest")
            .field("name", &self.name)
            .field("url_host", &redacted_url(&self.url))
            .finish()
    }
}

/// Body of `PATCH …/calendar/feeds/{feed_id}`. Absent fields are unchanged.
#[derive(Clone, PartialEq, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateCalendarFeedRequest {
    /// New display name.
    pub name: Option<String>,
    /// New feed URL; triggers a re-sync.
    pub url: Option<String>,
    /// Switch syncing on or off.
    pub is_active: Option<bool>,
}

impl fmt::Debug for UpdateCalendarFeedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateCalendarFeedRequest")
            .field("name", &self.name)
            .field("url_host", &self.url.as_deref().map(redacted_url))
            .field("is_active", &self.is_active)
            .finish()
    }
}

/// Why a feed name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FeedNameError {
    /// Nothing left after trimming.
    #[error("the calendar name is required")]
    Empty,
    /// Longer than 60 characters.
    #[error("the calendar name must be at most 60 characters")]
    TooLong,
}

/// A feed name with control characters removed and whitespace trimmed.
pub fn clean_feed_name(raw: &str) -> Result<String, FeedNameError> {
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).collect();
    let cleaned = cleaned.trim();
    match cleaned.chars().count() {
        0 => Err(FeedNameError::Empty),
        n if n > MAX_FEED_NAME_CHARS => Err(FeedNameError::TooLong),
        _ => Ok(cleaned.to_string()),
    }
}

/// A blocked date range on a listing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BlockedDateResponse {
    /// Block id.
    pub id: Uuid,
    /// Listing the block is on.
    pub listing_id: Uuid,
    /// Imported or manual.
    pub source: BlockSource,
    /// Feed the block came from, for imported blocks.
    pub feed_id: Option<Uuid>,
    /// That feed's name.
    pub feed_name: Option<String>,
    /// Whether that feed is still synced.
    pub feed_active: Option<bool>,
    /// First blocked night.
    pub date_from: NaiveDate,
    /// The night after the last blocked night.
    pub date_to: NaiveDate,
    /// Private note, for manual blocks.
    pub note: Option<String>,
    /// When the block was created.
    pub created_at: DateTime<Utc>,
}

/// Body of `POST …/calendar/blocks`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct NewBlockedDateRequest {
    /// First night to block.
    pub date_from: NaiveDate,
    /// The night after the last one to block.
    pub date_to: NaiveDate,
    /// Private note, at most 500 characters. Never exported.
    pub note: Option<String>,
}

/// Why a manual block was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BlockedDateError {
    /// Starts before today (Jamaica time).
    #[error("blocked dates can't start in the past")]
    InPast,
    /// `date_to` isn't after `date_from`.
    #[error("the end date must be after the start date")]
    EmptyRange,
    /// More than 365 nights.
    #[error("a block can be at most 365 nights")]
    TooLong,
    /// Ends more than 24 months ahead.
    #[error("blocked dates can be at most 24 months ahead")]
    TooFar,
    /// The note is longer than 500 characters.
    #[error("the note must be at most 500 characters")]
    NoteTooLong,
}

impl NewBlockedDateRequest {
    /// Checks the request against `today` (Jamaica time).
    pub fn validate_for(&self, today: NaiveDate) -> Result<(), BlockedDateError> {
        let horizon = today
            .checked_add_months(Months::new(FEED_WINDOW_MONTHS))
            .unwrap_or(NaiveDate::MAX);
        let nights = (self.date_to - self.date_from).num_days();
        match () {
            _ if self.date_from < today => Err(BlockedDateError::InPast),
            _ if nights <= 0 => Err(BlockedDateError::EmptyRange),
            _ if nights > MAX_BLOCK_NIGHTS => Err(BlockedDateError::TooLong),
            _ if self.date_to > horizon => Err(BlockedDateError::TooFar),
            _ if self
                .note
                .as_deref()
                .is_some_and(|n| n.chars().count() > MAX_BLOCK_NOTE_CHARS) =>
            {
                Err(BlockedDateError::NoteTooLong)
            }
            _ => Ok(()),
        }
    }
}

/// An imported block that collides with a stay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CalendarConflictResponse {
    /// Conflict id.
    pub id: Uuid,
    /// Listing it's on.
    pub listing_id: Uuid,
    /// Feed the block came from; `None` once the feed is removed.
    pub feed_id: Option<Uuid>,
    /// Feed name when the conflict was recorded.
    pub feed_name: String,
    /// The block; `None` once its event left the feed.
    pub block_id: Option<Uuid>,
    /// The booking it collides with.
    pub booking_id: Uuid,
    /// That booking's reservation number.
    pub confirmation_code: String,
    /// The booking's check-in.
    pub booking_date_from: NaiveDate,
    /// The booking's check-out.
    pub booking_date_to: NaiveDate,
    /// Overlap or turnover.
    pub kind: ConflictKind,
    /// The block's first night when last detected.
    pub block_date_from: NaiveDate,
    /// The night after the block's last night when last detected.
    pub block_date_to: NaiveDate,
    /// When first detected.
    pub detected_at: DateTime<Utc>,
    /// When last reopened.
    pub reopened_at: Option<DateTime<Utc>>,
    /// Whether it reopened after being acknowledged or going stale.
    pub reopened: bool,
    /// When acknowledged.
    pub acknowledged_at: Option<DateTime<Utc>>,
    /// Who acknowledged it.
    pub acknowledged_by: Option<Uuid>,
    /// Why it no longer needs attention, if so.
    pub stale_reason: Option<StaleReason>,
}

/// One page of results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Page<T> {
    /// The page's items.
    pub items: Vec<T>,
    /// 1-based page number.
    pub page: i64,
    /// Items per page.
    pub per_page: i64,
    /// Items across all pages.
    pub total: i64,
}

/// Body of `GET …/calendar/conflicts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CalendarConflictsResponse {
    /// Unacknowledged conflicts: live ones first, then stale ones.
    pub open: Vec<CalendarConflictResponse>,
    /// Acknowledged conflicts, newest first.
    pub history: Page<CalendarConflictResponse>,
}

/// What happened to one feed in a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    /// Synced.
    Ok,
    /// Synced with a warning.
    Warning,
    /// Fetching or parsing failed; blocks were kept.
    Failed,
    /// Synced too recently, so not attempted.
    Skipped,
    /// Ran out of time or waited too long for a lock; retried later.
    Deferred,
}

/// One feed's result in a sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FeedSyncResult {
    /// The feed.
    pub feed_id: Uuid,
    /// What happened.
    pub outcome: SyncOutcome,
    /// Events imported, when the sync succeeded.
    pub events: Option<i32>,
    /// Sanitised error or warning code.
    pub error_code: Option<String>,
    /// Conflicts that opened or reopened.
    pub conflicts_opened: i32,
}

/// Body of `POST …/calendar/feeds`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CreateCalendarFeedResponse {
    /// The saved feed, after its first sync.
    pub feed: CalendarFeedResponse,
    /// The first sync's result.
    pub sync: FeedSyncResult,
}

/// Body of `PATCH …/calendar/feeds/{feed_id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct UpdateCalendarFeedResponse {
    /// The feed after the change.
    pub feed: CalendarFeedResponse,
    /// The re-sync's result, when the URL changed.
    pub sync: Option<FeedSyncResult>,
}

/// One listing's calendar problems, for the dashboard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CalendarOverviewListing {
    /// The listing.
    pub listing_id: Uuid,
    /// Its name.
    pub listing_name: String,
    /// Open, non-stale conflicts.
    pub open_conflicts: i64,
    /// Feeds that failed 4 scheduled syncs or more in a row.
    pub failing_feeds: i64,
}

/// Calendar problems across the caller's listings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ToSchema)]
pub struct CalendarOverview {
    /// Open, non-stale conflicts in total.
    pub open_conflicts: i64,
    /// Failing feeds in total.
    pub failing_feeds: i64,
    /// Listings with at least one problem.
    pub listings: Vec<CalendarOverviewListing>,
}
