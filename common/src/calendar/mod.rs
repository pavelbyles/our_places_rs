//! iCal calendar sync (#58): feed and block types, feed parsing and writing,
//! feed URL rules and conflict classification. Pure code with no IO.

mod types;

pub use types::{BlockSource, ConflictKind, FeedStatus};
