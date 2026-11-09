//! iCal calendar sync storage (#58): imported feeds, blocked dates, conflicts
//! and the export query.

/// Blocked dates: manual blocks and the shared overlap check.
pub mod blocks;
/// Conflicts between imported blocks and stays.
pub mod conflicts;
/// Imported feeds: storage, edits and claiming.
pub mod feeds;
/// Applying a fetched feed, and recording failures.
pub mod reconcile;

#[cfg(test)]
mod block_tests;
#[cfg(test)]
mod conflict_tests;
#[cfg(test)]
mod feeds_tests;
#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod reconcile_tests;
