//! Where the map tiles are served from (spec 103 §1.10, "Portal
//! configuration"). Read once at startup; maps are an add-on, so a missing
//! or invalid value turns them off with one `warn` and never stops a portal.

use std::env;

use thiserror::Error;
use topcoat::context::{Cx, try_context};

/// Full URL of one tile version folder, e.g.
/// `https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113`.
pub static MAP_TILES_BASE_URL_ENV: &str = "MAP_TILES_BASE_URL";

/// Cloud Storage's public host.
const GCS_HOST: &str = "https://storage.googleapis.com";

/// Why a `MAP_TILES_BASE_URL` value is not used.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MapConfigError {
    /// The variable is unset or empty.
    #[error("MAP_TILES_BASE_URL is not set; maps are off")]
    Unset,
    /// The value is not a tile version folder URL.
    #[error("MAP_TILES_BASE_URL is not a tile folder URL; maps are off")]
    Invalid,
}

/// A validated tile folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapConfig {
    base_url: String,
    origin: String,
}

/// A Cloud Storage bucket name: 3–63 of `a-z 0-9 . _ -`, starting and
/// ending with a letter or digit, and no `..`.
fn is_bucket(name: &str) -> bool {
    let bytes = name.as_bytes();
    let edge = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    (3..=63).contains(&bytes.len())
        && bytes.first().is_some_and(edge)
        && bytes.last().is_some_and(edge)
        && bytes
            .iter()
            .all(|b| edge(b) || matches!(b, b'.' | b'_' | b'-'))
        && !name.contains("..")
}

/// `v` and eight digits (a Protomaps build date).
fn is_version(segment: &str) -> bool {
    segment
        .strip_prefix('v')
        .is_some_and(|d| d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit()))
}

/// An emulator endpoint safe to place in a CSP: `http(s)://` and then only
/// host and port characters.
fn is_emulator_host(endpoint: &str) -> bool {
    ["http://", "https://"].iter().any(|scheme| {
        endpoint.strip_prefix(scheme).is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
        })
    })
}

impl MapConfig {
    /// Validates `raw` (spec 103 §1.10: an allowlist, nothing is cleaned
    /// up). `emulator` is the local Cloud Storage emulator's endpoint, if
    /// one is configured; only then is the same shape under it accepted.
    pub fn parse(raw: &str, emulator: Option<&str>) -> Result<Self, MapConfigError> {
        if raw.is_empty() {
            return Err(MapConfigError::Unset);
        }
        let hosts = std::iter::once(GCS_HOST).chain(emulator.filter(|e| is_emulator_host(e)));
        hosts
            .filter_map(|host| {
                raw.strip_prefix(host)
                    .and_then(|rest| rest.strip_prefix('/'))
                    .map(|rest| (host, rest))
            })
            .find_map(|(host, rest)| {
                rest.split_once('/')
                    .filter(|(bucket, version)| is_bucket(bucket) && is_version(version))
                    .map(|(bucket, _)| Self {
                        base_url: raw.to_string(),
                        origin: format!("{host}/{bucket}/"),
                    })
            })
            .ok_or(MapConfigError::Invalid)
    }

    /// Reads [`MAP_TILES_BASE_URL_ENV`] through `lookup`, logging one `warn`
    /// when maps end up off.
    pub fn from_lookup(
        lookup: impl Fn(&str) -> Option<String>,
        emulator: Option<&str>,
    ) -> Option<Self> {
        let raw = lookup(MAP_TILES_BASE_URL_ENV).unwrap_or_default();
        Self::parse(&raw, emulator)
            .inspect_err(|e| match e {
                MapConfigError::Unset => tracing::warn!("{e}"),
                MapConfigError::Invalid => tracing::warn!(value = ?raw, "{e}"),
            })
            .ok()
    }

    /// Reads the environment (see [`MapConfig::from_lookup`]).
    pub fn from_env() -> Option<Self> {
        let emulator = common::gcs::emulator_endpoint();
        Self::from_lookup(|key| env::var(key).ok(), emulator.as_deref())
    }

    /// The tile folder URL, with no trailing `/`.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The bucket path the CSP allows, with a trailing `/`, e.g.
    /// `https://storage.googleapis.com/our-places-gcs-map-tiles-dev/`.
    pub fn origin(&self) -> &str {
        &self.origin
    }
}

/// The portal's map configuration, if maps are on. Portals register it on
/// the router (`.with(..)`) only when [`MapConfig::from_env`] returns one.
pub fn map_config(cx: &Cx) -> Option<&MapConfig> {
    try_context::<MapConfig>(cx)
}

#[cfg(test)]
#[path = "map_config_tests.rs"]
mod map_config_tests;

/// Test helpers: a context with maps on.
#[cfg(any(test, feature = "test-util"))]
pub mod testing {
    use super::MapConfig;
    use crate::assets::{MAP_JS, MAPLIBRE_CSS, MAPLIBRE_JS, MAPLIBRE_WORKER_JS, PMTILES_JS};
    use topcoat::asset::{Asset, AssetConfig, Manifest, ManifestEntry};
    use topcoat::context::Cx;

    /// The tile folder the helpers use.
    pub const TEST_TILES: &str =
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113";

    fn entry(asset: Asset, file: &str) -> ManifestEntry {
        ManifestEntry {
            id: asset.id(),
            file: file.to_string(),
            hash: "0".to_string(),
            content_type: "application/javascript".to_string(),
        }
    }

    /// A context with maps on ([`TEST_TILES`]) and the map assets hosted at
    /// `/_topcoat/assets`.
    ///
    /// # Panics
    /// If [`TEST_TILES`] stops parsing: a test helper, never production.
    pub fn maps_on() -> Cx {
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
        let config = match MapConfig::parse(TEST_TILES, None) {
            Ok(config) => config,
            Err(e) => panic!("{e}"),
        };
        Cx::default()
            .with(AssetConfig::hosted_at("/_topcoat/assets", manifest))
            .with(config)
    }
}
