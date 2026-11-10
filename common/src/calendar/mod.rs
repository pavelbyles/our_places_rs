//! iCal calendar sync (#58): feed and block types, feed parsing and writing,
//! feed URL rules and conflict classification. Pure code with no IO.

/// Conflict classification and stale reasons.
pub mod conflict;
/// API request and response types.
pub mod dto;
/// Feed URL validation, the IP blocklist and host-only redaction.
pub mod feed_url;
/// The `.ics` feed parser.
pub mod ics_parse;
/// The busy-only `.ics` export writer and its ETag.
pub mod ics_write;
mod types;

#[cfg(test)]
mod conflict_tests;
#[cfg(test)]
mod dto_tests;
#[cfg(test)]
mod feed_url_tests;
#[cfg(test)]
mod ics_parse_tests;
#[cfg(test)]
mod ics_write_tests;

pub use types::{BlockSource, ConflictKind, FeedStatus};
