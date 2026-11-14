//! Where a listing is, and how precisely guests may see it (spec 103 §1.1–1.3).
//!
//! A listing is either `exact` or `approximate`. For an approximate listing,
//! public responses carry a point moved 150–500 m from the stored one. The
//! offset is fixed per listing and derived from a secret key, so it can't be
//! averaged away over repeated requests or recomputed from the public listing ID.
//!
//! Pure functions only; nothing here logs or stores coordinates.

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

/// Shortest distance, in metres, between a stored point and its public point.
pub const APPROX_MIN_M: f64 = 150.0;

/// Longest distance, in metres, between a stored point and its public point.
/// Also the radius of the circle guests see, so the stored point is always
/// inside it.
pub const APPROX_MAX_M: f64 = 500.0;

/// Decimal places kept on a public coordinate (about 1.1 m).
pub const APPROX_DECIMALS: i32 = 5;

/// Fewest bytes a location offset key may have.
pub const MIN_KEY_BYTES: usize = 32;

/// Mean Earth radius in metres, as used by haversine distances.
pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Metres per degree of latitude on a sphere of [`EARTH_RADIUS_M`].
const METRES_PER_DEGREE: f64 = EARTH_RADIUS_M * std::f64::consts::PI / 180.0;

/// A latitude/longitude box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Western edge (longitude).
    pub west: f64,
    /// Southern edge (latitude).
    pub south: f64,
    /// Eastern edge (longitude).
    pub east: f64,
    /// Northern edge (latitude).
    pub north: f64,
}

/// The area the map covers: Jamaica, with a margin.
pub const JAMAICA_BOUNDS: Bounds = Bounds {
    west: -78.60,
    south: 17.55,
    east: -75.95,
    north: 18.70,
};

/// Whether a point lies within [`JAMAICA_BOUNDS`], edges included. Non-finite
/// values are never in bounds.
pub fn in_bounds(latitude: f64, longitude: f64) -> bool {
    let b = JAMAICA_BOUNDS;
    latitude.is_finite()
        && longitude.is_finite()
        && (b.south..=b.north).contains(&latitude)
        && (b.west..=b.east).contains(&longitude)
}

/// Checks a listing's coordinate input (spec 103 §1.1.4): both or neither,
/// finite, latitude within ±90 and longitude within ±180. Errors are keyed by
/// field, as `validator` reports them.
pub fn validate_coordinates(
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> Result<(), validator::ValidationErrors> {
    let mut errors = validator::ValidationErrors::new();
    let mut refuse = |field: &'static str, code: &'static str, message: &'static str| {
        errors.add(
            field,
            validator::ValidationError::new(code).with_message(message.into()),
        );
    };
    match (latitude, longitude) {
        (Some(_), None) => refuse(
            "longitude",
            "coordinates_together",
            "Latitude and longitude must be given together.",
        ),
        (None, Some(_)) => refuse(
            "latitude",
            "coordinates_together",
            "Latitude and longitude must be given together.",
        ),
        _ => {}
    }
    if let Some(lat) = latitude {
        match lat.is_finite() && (-90.0..=90.0).contains(&lat) {
            true => {}
            false => refuse(
                "latitude",
                "latitude_range",
                "Latitude must be a number between -90 and 90.",
            ),
        }
    }
    if let Some(lng) = longitude {
        match lng.is_finite() && (-180.0..=180.0).contains(&lng) {
            true => {}
            false => refuse(
                "longitude",
                "longitude_range",
                "Longitude must be a number between -180 and 180.",
            ),
        }
    }
    match errors.is_empty() {
        true => Ok(()),
        false => Err(errors),
    }
}

/// What a host lets guests see before they book.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema,
)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "location_privacy", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum LocationPrivacy {
    /// Guests see a 500 m area around a moved point.
    #[default]
    Approximate,
    /// Guests see the stored point.
    Exact,
}

impl LocationPrivacy {
    /// The stored and form value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approximate => "approximate",
            Self::Exact => "exact",
        }
    }
}

/// How precise the coordinates in a response are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocationPrecision {
    /// A point moved from the real one; draw an area, not a pin.
    Approximate,
    /// The real point.
    Exact,
}

impl From<LocationPrivacy> for LocationPrecision {
    fn from(privacy: LocationPrivacy) -> Self {
        match privacy {
            LocationPrivacy::Approximate => Self::Approximate,
            LocationPrivacy::Exact => Self::Exact,
        }
    }
}

/// A listing's stored coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListingLocation {
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
}

/// A listing's stored location and setting, as its managers see it
/// (`GET /api/v1/listings/{id}/location`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListingLocationResponse {
    /// Stored latitude; `None` unless both coordinates are stored.
    pub latitude: Option<f64>,
    /// Stored longitude; `None` unless both coordinates are stored.
    pub longitude: Option<f64>,
    /// What guests may see before they book.
    pub location_privacy: LocationPrivacy,
}

/// Why a location offset key was refused. Never carries the value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LocationKeyError {
    /// No key, or only whitespace.
    #[error("the location offset key is missing")]
    Missing,
    /// Fewer than [`MIN_KEY_BYTES`] bytes.
    #[error("the location offset key is shorter than {MIN_KEY_BYTES} bytes")]
    TooShort,
    /// The key could not initialise HMAC-SHA256.
    #[error("the location offset key could not be used")]
    Unusable,
}

/// The secret that fixes each approximate listing's offset. Its `Debug`
/// output never shows the key.
#[derive(Clone)]
pub struct LocationKey {
    mac: Hmac<Sha256>,
}

impl std::fmt::Debug for LocationKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocationKey(**redacted**)")
    }
}

impl LocationKey {
    /// Builds a key from its configured text, trimmed. It must be at least
    /// [`MIN_KEY_BYTES`] bytes.
    pub fn from_secret(raw: &str) -> Result<Self, LocationKeyError> {
        Some(raw.trim())
            .filter(|key| !key.is_empty())
            .ok_or(LocationKeyError::Missing)
            .and_then(|key| match key.len() >= MIN_KEY_BYTES {
                true => Ok(key),
                false => Err(LocationKeyError::TooShort),
            })
            .and_then(|key| {
                Hmac::<Sha256>::new_from_slice(key.as_bytes())
                    .map_err(|_| LocationKeyError::Unusable)
            })
            .map(|mac| Self { mac })
    }

    /// The 32-byte HMAC of `listing_id`.
    fn seed(&self, listing_id: Uuid) -> [u8; 32] {
        let mut mac = self.mac.clone();
        mac.update(listing_id.as_bytes());
        mac.finalize().into_bytes().into()
    }
}

/// A uniform value in [0, 1) from the top 53 bits of eight seed bytes.
fn unit(bytes: [u8; 8]) -> f64 {
    (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
}

/// `value` rounded to [`APPROX_DECIMALS`] places.
fn round_coordinate(value: f64) -> f64 {
    let scale = 10f64.powi(APPROX_DECIMALS);
    (value * scale).round() / scale
}

/// `longitude` wrapped into [−180, 180).
fn wrap_longitude(longitude: f64) -> f64 {
    (longitude + 180.0).rem_euclid(360.0) - 180.0
}

/// The public point of an approximate listing: the stored point moved by an
/// offset fixed by `key` and `listing_id` (spec 103 §1.3).
///
/// The bearing is uniform, and the distance is area-uniform within
/// [`APPROX_MIN_M`, `APPROX_MAX_M`]. The result is rounded to
/// [`APPROX_DECIMALS`] places, then latitude is clamped and longitude wrapped.
pub fn approximate_point(
    latitude: f64,
    longitude: f64,
    key: &LocationKey,
    listing_id: Uuid,
) -> (f64, f64) {
    let seed = key.seed(listing_id);
    let take = |start: usize| -> [u8; 8] { std::array::from_fn(|i| seed[start + i]) };
    let bearing = 2.0 * std::f64::consts::PI * unit(take(0));
    let (min_sq, max_sq) = (APPROX_MIN_M.powi(2), APPROX_MAX_M.powi(2));
    let distance = (min_sq + unit(take(8)) * (max_sq - min_sq)).sqrt();

    let delta_lat = distance * bearing.cos() / METRES_PER_DEGREE;
    // Guard the pole, where a degree of longitude has no width.
    let lng_scale = latitude.to_radians().cos().abs().max(1e-6);
    let delta_lng = distance * bearing.sin() / (METRES_PER_DEGREE * lng_scale);

    let lat = round_coordinate(latitude + delta_lat).clamp(-90.0, 90.0);
    let lng = round_coordinate(wrap_longitude(round_coordinate(longitude + delta_lng)));
    (lat, lng)
}

#[cfg(test)]
#[path = "location_tests.rs"]
mod location_tests;
