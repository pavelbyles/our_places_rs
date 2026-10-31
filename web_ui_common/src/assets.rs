use topcoat::asset::{Asset, asset};

/// The portals' only first-party script: dialogs, dismissal, theme toggle,
/// time-zone cookie, print and toasts, all wired through `data-*` attributes.
pub const UI_JS: Asset = asset!("src/assets/ui.js");

/// htmx 2.0.4, vendored (the legacy portals load the same version from a
/// CDN, which `script-src 'self'` forbids). Verified against the npm tarball
/// (sha512 integrity HLxMCdfXDOJirs3vBZl/ZLoY+c7PfM4Ahr2Ad4YXh6d22T5ltbTXFFkpx9Tgb2vvmWFMbIc3LqN2ToNkZJvyYQ==).
pub const HTMX_JS: Asset = asset!(
    "src/assets/htmx-2.0.4.min.js",
    checksum: "sha256:e209dda5c8235479f3166defc7750e1dbcd5a5c1808b7792fc2e6733768fb447",
);

/// `<meta name="htmx-config">` content: no injected indicator `<style>`
/// (style-src 'self'), no eval, no script tags in swapped content.
pub const HTMX_CONFIG: &str =
    r#"{"includeIndicatorStyles":false,"allowEval":false,"allowScriptTags":false}"#;

/// Home page hero photograph.
pub const HERO_IMAGE: Asset = asset!("src/assets/hero.jpg");
