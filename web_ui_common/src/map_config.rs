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
