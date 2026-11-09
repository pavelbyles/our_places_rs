//! iCal calendar sync (#58): feed and block types, feed parsing and writing,
//! feed URL rules and conflict classification. Pure code with no IO.

/// Feed URL validation, the IP blocklist and host-only redaction.
pub mod feed_url;
mod types;

#[cfg(test)]
mod feed_url_tests;

pub use types::{BlockSource, ConflictKind, FeedStatus};
