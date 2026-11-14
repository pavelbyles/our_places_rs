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

// ---- Map view (spec 103 §1.11). Vendored verbatim from the npm tarballs
// (registry integrity checked); loaded lazily by `ui.js` only when a map
// container is about to be shown. Licences: MapLibre GL JS BSD-3-Clause
// (`maplibre-gl-LICENSE.txt`), pmtiles BSD-3-Clause (package.json; the
// tarball ships no licence file).

/// MapLibre GL JS 6.15.0, ES module (`dist/maplibre-gl.mjs`). Loaded with
/// `import()`; it has no imports of its own.
pub const MAPLIBRE_JS: Asset = asset!(
    "src/assets/maplibre-gl-6.15.0.mjs",
    checksum: "sha256:41198b23679a7d8892183408bbef2eabdc74745beb90261f6be0f660b7aeb2fd",
);

/// MapLibre's module worker (`dist/maplibre-gl-worker.mjs`), passed to
/// `setWorkerUrl`: its default URL is the unhashed sibling file.
pub const MAPLIBRE_WORKER_JS: Asset = asset!(
    "src/assets/maplibre-gl-worker-6.15.0.mjs",
    checksum: "sha256:1c69066368331aae63967ed24a9affcf962cf55fcc6d8f532a8b040f736dbcfa",
);

/// MapLibre's stylesheet (`dist/maplibre-gl.css`).
pub const MAPLIBRE_CSS: Asset = asset!(
    "src/assets/maplibre-gl-6.15.0.css",
    checksum: "sha256:a90798a1953017790e590f7e1bd35ff103a8a4263972cb47290b78adf86f8c20",
);

/// pmtiles 4.5.0, the IIFE build (`dist/pmtiles.js`, global `pmtiles`); the
/// ESM build imports `fflate`.
pub const PMTILES_JS: Asset = asset!(
    "src/assets/pmtiles-4.5.0.js",
    checksum: "sha256:caf981bc46f6327ee7e65d5dc964d89d38a69f60edca2bd4c5c890c21b554c6c",
);

/// Ours: map setup for every `[data-map]` container (`OurPlacesMap.init`).
pub const MAP_JS: Asset = asset!("src/assets/map.js");
