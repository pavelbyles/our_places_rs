//! Listings: creation, search, updates, images and price overrides.

use uuid::Uuid;

mod images;
mod price_overrides;
mod queries;
mod write;
pub use images::{
    create_listing_image_presigns, delete_listing_image, insert_listing_image_variants,
    mark_listing_image_processed, update_listing_image_to_processing,
};
pub use price_overrides::{
    create_price_override, delete_price_override, get_active_overrides_for_dates,
    get_active_overrides_for_listings, get_price_override_by_id, get_price_overrides_by_listing,
    update_price_override,
};
pub use queries::{
    get_listing_by_id, get_listing_by_id_or_slug, get_listings, get_listings_by_user_id,
};
pub use write::{
    create_listing, create_listing_with_active_currency, delete_listing, update_listing,
    update_listing_with_conn,
};

/// A URL handle from a listing's title.
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
