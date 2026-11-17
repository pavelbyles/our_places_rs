//! Search-engine output for the guest portal (spec 102): which paths are
//! private, robots.txt, the sitemap and listing structured data.

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
