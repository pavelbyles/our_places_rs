//! iCal calendar sync storage (#58): imported feeds, blocked dates, conflicts
//! and the export query.

/// Blocked dates: manual blocks and the shared overlap check.
pub mod blocks;
/// Imported feeds: storage, edits and claiming.
pub mod feeds;

#[cfg(test)]
mod feeds_tests;
#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod migration_tests;
