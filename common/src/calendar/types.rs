//! Enums stored in the calendar sync tables.

use serde::{Deserialize, Serialize};

/// Where a blocked date range came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "calendar_block_source", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum BlockSource {
    /// Imported from an external `.ics` feed.
    IcalFeed,
    /// Added by hand by a host or the admin.
    Manual,
}

/// Outcome of a feed's last sync attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "calendar_feed_status", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum FeedStatus {
    /// Never synced.
    Never,
    /// The last sync succeeded.
    Ok,
    /// The last sync succeeded with a warning.
    Warning,
    /// The last sync failed.
    Failed,
}

/// How an imported block collides with a stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "calendar_conflict_kind", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// The block and the stay share at least one night.
    Overlap,
    /// They don't share a night, but the turnover gap is breached.
    Turnover,
}
