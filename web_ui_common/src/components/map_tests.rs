//! Spec 103 §1.11: the container renders nothing when maps are off, and
//! otherwise carries the hashed asset URLs and a fixed height.

use super::*;
use crate::map_config::MapConfig;
use topcoat::asset::{Asset, AssetConfig, Manifest, ManifestEntry};
use topcoat::view::ViewExt;

const TILES: &str = "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113";

async fn html(v: impl View, cx: &Cx) -> String {
    match v.first().await {
        Ok(handle) => handle.render(cx),
        Err(e) => panic!("render failed: {e}"),
    }
}

fn entry(asset: Asset, file: &str) -> ManifestEntry {
    ManifestEntry {
        id: asset.id(),
        file: file.to_string(),
        hash: "0".to_string(),
        content_type: "application/javascript".to_string(),
    }
}

/// A context with maps on and the map assets hosted at `/_topcoat/assets`.
fn maps_on() -> Cx {
    let manifest = Manifest {
        version: 1,
        assets: vec![
            entry(MAPLIBRE_JS, "maplibre-gl-6.15.0-aaaa.mjs"),
            entry(MAPLIBRE_WORKER_JS, "maplibre-gl-worker-6.15.0-bbbb.mjs"),
            entry(MAPLIBRE_CSS, "maplibre-gl-6.15.0-cccc.css"),
            entry(PMTILES_JS, "pmtiles-4.5.0-dddd.js"),
            entry(MAP_JS, "map-eeee.js"),
        ],
    };
    let config = match MapConfig::parse(TILES, None) {
        Ok(config) => config,
        Err(e) => panic!("{e}"),
    };
    Cx::default()
        .with(AssetConfig::hosted_at("/_topcoat/assets", manifest))
        .with(config)
}

#[tokio::test]
async fn renders_nothing_when_maps_are_off() {
    // No MapConfig and no asset config: nothing is rendered and nothing
    // asks for an asset URL.
    let cx = &Cx::default();
    let out = html(view! { map_container(kind: MapKind::Listing) }, cx).await;
    assert_eq!(out.trim(), "", "{out}");
}

#[tokio::test]
async fn carries_the_hashed_urls_the_tiles_and_a_fixed_height() {
    let cx = &maps_on();
    let point = MapPoint {
        lat: 18.40741,
        lng: -77.10313,
        precision: LocationPrecision::Approximate,
    };
    let out = html(
        view! { map_container(kind: MapKind::Listing, point: Some(point), label: Some("Where you'll be".to_string())) },
        cx,
    )
    .await;
    for expected in [
        r#"data-map="listing""#,
        r#"data-map-js="/_topcoat/assets/maplibre-gl-6.15.0-aaaa.mjs""#,
        r#"data-map-worker="/_topcoat/assets/maplibre-gl-worker-6.15.0-bbbb.mjs""#,
        r#"data-map-css="/_topcoat/assets/maplibre-gl-6.15.0-cccc.css""#,
        r#"data-pmtiles-js="/_topcoat/assets/pmtiles-4.5.0-dddd.js""#,
        r#"data-map-init="/_topcoat/assets/map-eeee.js""#,
        r#"data-tiles-base="https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113""#,
        r#"data-lat="18.40741""#,
        r#"data-lng="-77.10313""#,
        r#"data-precision="approximate""#,
        r#"aria-label="Where you'll be""#,
        "h-56",
        "data-map-canvas",
        "Map unavailable",
    ] {
        assert!(out.contains(expected), "missing {expected}: {out}");
    }
}

#[tokio::test]
async fn a_given_height_replaces_the_default_and_no_point_means_no_coordinates() {
    let cx = &maps_on();
    let out = html(
        view! { map_container(kind: MapKind::Search, attrs: topcoat::view::attributes! { class="h-96" }) },
        cx,
    )
    .await;
    assert!(out.contains("h-96"), "{out}");
    assert!(!out.contains("data-lat"), "{out}");
    assert!(!out.contains("data-precision"), "{out}");
    assert!(out.contains(r#"data-map="search""#), "{out}");
}

#[test]
fn kinds_match_map_js() {
    let kinds = [
        MapKind::Search,
        MapKind::Listing,
        MapKind::Booking,
        MapKind::Picker,
    ]
    .map(MapKind::as_str);
    assert_eq!(kinds, ["search", "listing", "booking", "picker"]);
}

#[tokio::test]
async fn says_it_needs_javascript_when_scripts_are_off() {
    let cx = &maps_on();
    let out = html(view! { map_container(kind: MapKind::Listing) }, cx).await;
    let start = out
        .find("<noscript>")
        .unwrap_or_else(|| panic!("no noscript: {out}"));
    let end = out
        .find("</noscript>")
        .unwrap_or_else(|| panic!("unclosed: {out}"));
    assert!(out[start..end].contains("Map needs JavaScript"), "{out}");
}
