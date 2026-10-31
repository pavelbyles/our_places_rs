//! Listings: creation, search, updates, images and price overrides.

use crate::error::{DbError, Result};
use crate::models::{Listing, ListingWithOwner, NewListing, UpdatedListing};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

mod images;
mod price_overrides;
mod queries;
mod write;
pub use images::*;
pub use price_overrides::*;
pub use queries::*;
pub use write::*;

pub fn generate_listing_slug_native(title: &str) -> String {
    const MAX_SLUG_LEN: usize = 60;
    let mut slug = String::with_capacity(MAX_SLUG_LEN);
    let mut last_was_hyphen = false;

    for c in title.to_lowercase().chars() {
        if slug.len() >= MAX_SLUG_LEN {
            break;
        }

        if c.is_ascii_alphanumeric() {
            slug.push(c);
            last_was_hyphen = false;
        } else if !last_was_hyphen && !slug.is_empty() {
            slug.push('-');
            last_was_hyphen = true;
        }
    }

    let trimmed = slug.trim_end_matches('-');

    if Uuid::parse_str(trimmed).is_ok() {
        format!("v-{}", trimmed)
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests;
