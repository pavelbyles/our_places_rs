//! Search-engine output for the guest portal (spec 102): which paths are
//! private, robots.txt, the sitemap and listing structured data.

use std::fmt;
use std::time::SystemTime;

use common::app_client::ListingSearchParams;
use common::models::ListingResponse;
use topcoat::context::BaseUrl;
use topcoat::router::content::sitemap::{Sitemap, SitemapUrl};
use web_client::TopcoatApiClient;
use web_ui_common::shell_guest::POLICY_LINKS;

/// `<title>`s of the static pages.
pub mod titles;

/// robots.txt for an environment search engines must stay out of.
pub const DISALLOW_ALL: &str = "User-agent: *\nDisallow: /\n";

/// robots.txt `Disallow` rules for the guest portal (spec 102 §1.3): sign-in
/// and account pages, checkout and booking flows, and listing sub-routes
/// that are not content. `*` matches any run of characters, and each rule is
/// a prefix (`/listings/*/calendar` also covers `calendar.ics`). These paths
/// also stay `X-Robots-Tag: noindex` on an indexable site (§1.6).
pub const PRIVATE_PREFIXES: &[&str] = &[
    "/checkout/",
    "/bookings",
    "/profile",
    "/requests",
    "/reviews/",
    "/login",
    "/logout",
    "/register",
    "/verify",
    "/currency/",
    "/listings/*/quote",
    "/listings/*/enquire",
    "/listings/*/calendar",
];

/// The guest robots.txt (spec 102 §1.3). Not indexable: everything is
/// disallowed and no sitemap is named. Indexable: every
/// [`PRIVATE_PREFIXES`] rule is disallowed, everything else (including
/// assets, which pages need to render) is allowed, and the sitemap is named
/// by its absolute URL.
pub fn robots_txt(base: &BaseUrl, indexable: bool) -> String {
    if !indexable {
        return DISALLOW_ALL.to_string();
    }
    let disallows: String = PRIVATE_PREFIXES
        .iter()
        .map(|rule| format!("Disallow: {rule}\n"))
        .collect();
    format!(
        "User-agent: *\n{disallows}Allow: /\n\nSitemap: {}\n",
        base.join("/sitemap.xml")
    )
}

/// Longest page description, in characters (search engines show about
/// this much).
pub const META_DESCRIPTION_CHARS: usize = 160;

/// `text` cut to at most `max_chars` characters (not bytes, so multi-byte
/// text never splits a character), at the last word boundary, with `…`.
/// Text that fits is returned as is.
pub fn truncate_on_word(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    let cut = match head.rfind(char::is_whitespace) {
        Some(at) if at > 0 => &head[..at],
        _ => head.as_str(),
    };
    format!(
        "{}…",
        cut.trim_end_matches(|c: char| c.is_whitespace() || c == ',')
    )
}

/// A listing's `<meta name="description">` (spec 102 §1.5.4): its own
/// description, whitespace collapsed and cut at a word within
/// [`META_DESCRIPTION_CHARS`]; without one, "3-bedroom villa in Negril,
/// Jamaica" (bedrooms left out when 0, city when unknown).
pub fn meta_description(listing: &ListingResponse) -> String {
    let own = listing
        .description
        .as_deref()
        .map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|d| !d.is_empty());
    match own {
        Some(text) => truncate_on_word(&text, META_DESCRIPTION_CHARS),
        None => {
            let place = match listing
                .city
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty())
            {
                Some(city) => format!("{city}, {}", listing.country),
                None => listing.country.clone(),
            };
            let structure = listing.listing_structure.to_lowercase();
            if listing.bedrooms > 0 {
                format!("{}-bedroom {structure} in {place}", listing.bedrooms)
            } else {
                format!("{} in {place}", listing.listing_structure)
            }
        }
    }
}

/// Listings fetched per page while building the sitemap.
pub const SITEMAP_PAGE_SIZE: u32 = 100;

/// Most pages fetched for one sitemap (5000 listings). Reaching it is a
/// failure, not a cut-off: a partial sitemap would tell crawlers the missing
/// listings are gone (spec 102 §1.2).
pub const SITEMAP_MAX_PAGES: u32 = 50;

/// Public pages listed before the listings.
const STATIC_PATHS: [&str; 3] = ["/", "/listings", "/about"];

/// Why the sitemap could not be built.
#[derive(Debug)]
pub enum SitemapError {
    /// A page of the listing search failed.
    Api {
        /// The page that failed.
        page: u32,
        /// The client error.
        source: anyhow::Error,
    },
    /// Every page up to [`SITEMAP_MAX_PAGES`] was full.
    TooManyPages,
}

impl fmt::Display for SitemapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api { page, source } => write!(f, "listing search page {page} failed: {source}"),
            Self::TooManyPages => write!(
                f,
                "more than {} listings; the sitemap needs an index",
                SITEMAP_PAGE_SIZE * SITEMAP_MAX_PAGES
            ),
        }
    }
}

/// The search for one sitemap page: public listings only. Nothing but the
/// paging is set, so `include_inactive` stays `false` and no owner, host or
/// currency filter applies.
pub fn sitemap_search(page: u32) -> ListingSearchParams {
    ListingSearchParams {
        page: Some(page),
        per_page: Some(SITEMAP_PAGE_SIZE),
        ..ListingSearchParams::default()
    }
}

/// Every active, non-deleted listing, page by page until a short page.
pub async fn collect_listings(
    client: &TopcoatApiClient,
) -> Result<Vec<ListingResponse>, SitemapError> {
    let mut all = Vec::new();
    for page in 1..=SITEMAP_MAX_PAGES {
        let batch = client
            .search_listings(sitemap_search(page))
            .await
            .map_err(|source| SitemapError::Api { page, source })?;
        let full = batch.len() >= SITEMAP_PAGE_SIZE as usize;
        all.extend(batch);
        if !full {
            return Ok(all);
        }
    }
    Err(SitemapError::TooManyPages)
}

/// The sitemap: the public static pages, then one entry per listing, sorted
/// and deduplicated by slug, dated by `last_modified_at` when the API sent
/// it. Paths are resolved against the site's `BaseUrl` when rendered.
pub fn sitemap_for(mut listings: Vec<ListingResponse>) -> Sitemap {
    listings.sort_by(|a, b| a.slug.cmp(&b.slug));
    listings.dedup_by(|a, b| a.slug == b.slug);
    let pages = STATIC_PATHS
        .into_iter()
        .chain(POLICY_LINKS.iter().map(|(path, _)| *path))
        .map(SitemapUrl::new);
    let listings = listings.into_iter().map(|listing| {
        let url = SitemapUrl::new(format!("/listings/{}", listing.slug));
        match listing.last_modified_at {
            Some(at) => url.last_modified(SystemTime::from(at)),
            None => url,
        }
    });
    Sitemap::new().urls(pages).urls(listings)
}

#[cfg(test)]
mod seo_tests;

#[cfg(test)]
mod sitemap_tests;

#[cfg(test)]
mod page_titles_tests;
