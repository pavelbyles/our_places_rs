//! Search-engine output for the guest portal (spec 102): which paths are
//! private, robots.txt, the sitemap and listing structured data.

use topcoat::context::BaseUrl;

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

#[cfg(test)]
mod seo_tests;
