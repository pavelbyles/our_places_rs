//! The guest portal's public origin and SEO switches (spec 102 §1.1).
//!
//! Read once at startup. Unlike maps, the origin is not optional: canonical,
//! Open Graph and sitemap URLs are built from it, and Topcoat panics while
//! resolving a site-relative metadata URL without a [`BaseUrl`]. So a
//! missing or invalid `GUEST_PORTAL_URL` stops the guest portal before it
//! binds its port. The request's `Host` header is never used instead (it
//! would let a client poison canonical and sitemap URLs).

use std::env;

use thiserror::Error;
use topcoat::context::{BaseUrl, Cx, try_context};

/// The guest portal's public origin, e.g. `https://ourplaces.io`.
pub static GUEST_PORTAL_URL_ENV: &str = "GUEST_PORTAL_URL";

/// `true` or `1` lets search engines index the guest portal; anything else
/// (including unset) keeps it out of search results.
pub static SEO_INDEXABLE_ENV: &str = "SEO_INDEXABLE";

/// Optional X account for `twitter:site`, e.g. `@ourplaces`.
pub static SEO_TWITTER_SITE_ENV: &str = "SEO_TWITTER_SITE";

/// Site name shown in titles and Open Graph previews.
pub const SITE_NAME: &str = "Our Places";

/// Default page description.
pub const SITE_DESCRIPTION: &str = "Private luxury villas and apartments in Jamaica, hosted by their owners. Book direct with Our Places.";

/// Alternative text of the default Open Graph image.
pub const OG_DEFAULT_ALT: &str = "A villa on the Jamaican coast";

/// Title of pages that set none.
pub const DEFAULT_PAGE_TITLE: &str = "Luxury Jamaica Stays";

/// Longest X handle, without the `@`.
const MAX_HANDLE_CHARS: usize = 15;

/// Why the site configuration is rejected. Messages name the variable and
/// the rule, never the configured value.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SiteConfigError {
    /// `GUEST_PORTAL_URL` is unset or empty.
    #[error("GUEST_PORTAL_URL is missing")]
    Missing,
    /// Not an absolute `http(s)` URL, or it has a query or fragment.
    #[error("GUEST_PORTAL_URL is not an absolute http(s) URL without a query or fragment")]
    NotAbsolute,
    /// The URL has a path; the portal is served from the host root.
    #[error("GUEST_PORTAL_URL has a path")]
    HasPath,
    /// Indexing is on but the origin is not `https`.
    #[error("GUEST_PORTAL_URL must be https when SEO_INDEXABLE=true")]
    InsecureWhenIndexable,
}

/// The validated guest site configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteConfig {
    base_url: BaseUrl,
    indexable: bool,
    twitter_site: Option<String>,
}

/// `@` then 1–15 of `[A-Za-z0-9_]`.
fn is_handle(raw: &str) -> bool {
    raw.strip_prefix('@').is_some_and(|name| {
        (1..=MAX_HANDLE_CHARS).contains(&name.len())
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

fn parse_base_url(raw: &str, indexable: bool) -> Result<BaseUrl, SiteConfigError> {
    Some(raw)
        .filter(|r| !r.is_empty())
        .ok_or(SiteConfigError::Missing)
        .and_then(|r| BaseUrl::new(r).map_err(|_| SiteConfigError::NotAbsolute))
        .and_then(|base| {
            if base.prefix().is_empty() {
                Ok(base)
            } else {
                Err(SiteConfigError::HasPath)
            }
        })
        .and_then(|base| {
            if indexable && !base.as_str().starts_with("https://") {
                Err(SiteConfigError::InsecureWhenIndexable)
            } else {
                Ok(base)
            }
        })
}

impl SiteConfig {
    /// Reads and validates the variables through `lookup` (the environment
    /// in production). An invalid `SEO_TWITTER_SITE` is dropped with one
    /// `warn`; it never stops the portal.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, SiteConfigError> {
        let indexable = lookup(SEO_INDEXABLE_ENV).is_some_and(|v| v == "true" || v == "1");
        let twitter_site = lookup(SEO_TWITTER_SITE_ENV)
            .filter(|v| !v.is_empty())
            .and_then(|v| {
                if is_handle(&v) {
                    Some(v)
                } else {
                    tracing::warn!(
                        variable = SEO_TWITTER_SITE_ENV,
                        "not an @handle of 1-15 letters, digits or underscores; twitter:site is omitted"
                    );
                    None
                }
            });
        let raw = lookup(GUEST_PORTAL_URL_ENV).unwrap_or_default();
        parse_base_url(&raw, indexable).map(|base_url| Self {
            base_url,
            indexable,
            twitter_site,
        })
    }

    /// Reads the environment (see [`SiteConfig::from_lookup`]).
    pub fn from_env() -> Result<Self, SiteConfigError> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// The public origin, without a trailing `/`.
    pub fn base_url(&self) -> &BaseUrl {
        &self.base_url
    }

    /// Whether search engines may index the guest portal.
    pub fn indexable(&self) -> bool {
        self.indexable
    }

    /// The X account for `twitter:site`, with its `@`.
    pub fn twitter_site(&self) -> Option<&str> {
        self.twitter_site.as_deref()
    }
}

/// The guest site configuration, when the portal registered one on the
/// router (`.with(..)`).
pub fn site_config(cx: &Cx) -> Option<&SiteConfig> {
    try_context::<SiteConfig>(cx)
}

/// A page title with the site name appended: `"Villas · Our Places"`.
pub fn page_title(title: &str) -> String {
    format!("{title} · {SITE_NAME}")
}

#[cfg(test)]
#[path = "site_config_tests.rs"]
mod site_config_tests;
