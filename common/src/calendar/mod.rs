//! iCal calendar sync (#58): feed and block types, feed parsing and writing,
//! feed URL rules and conflict classification. Pure code with no IO.

/// Feed URL validation, the IP blocklist and host-only redaction.
pub mod feed_url;
/// The `.ics` feed parser.
pub mod ics_parse;
mod types;

#[cfg(test)]
mod feed_url_tests;
#[cfg(test)]
mod ics_parse_tests;

pub use types::{BlockSource, ConflictKind, FeedStatus};
