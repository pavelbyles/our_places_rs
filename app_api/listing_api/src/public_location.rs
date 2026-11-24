//! Public coordinates and the echo guard (spec 103 §1.1.5, §1.2).
//!
//! The only place a public path fills a listing's coordinates. Exact listings
//! show their stored point; approximate listings show the keyed offset point;
//! a listing missing either coordinate shows none. Nothing here logs a
//! coordinate.

use api_core::models::StoredLocation;
use common::location::{LocationKey, LocationPrecision, LocationPrivacy, approximate_point};
use common::models::ListingResponse;
use uuid::Uuid;

/// The point a guest who hasn't booked may see, and how precise it is.
pub fn public_point(
    stored: StoredLocation,
    key: &LocationKey,
    listing_id: Uuid,
) -> Option<((f64, f64), LocationPrecision)> {
    stored.point().map(|(lat, lng)| match stored.privacy {
        LocationPrivacy::Exact => ((lat, lng), LocationPrecision::Exact),
        LocationPrivacy::Approximate => (
            approximate_point(lat, lng, key, listing_id),
            LocationPrecision::Approximate,
        ),
    })
}

/// Fills `response` with public coordinates. Never sets `location_privacy`.
pub fn public_location(response: &mut ListingResponse, stored: StoredLocation, key: &LocationKey) {
    let public = public_point(stored, key, response.id);
    response.latitude = public.map(|((lat, _), _)| lat);
    response.longitude = public.map(|((_, lng), _)| lng);
    response.location_precision = public.map(|(_, precision)| precision);
    response.location_privacy = None;
}

/// Whether two coordinates are the same to the public point's precision.
fn same_to_five_places(a: f64, b: f64) -> bool {
    let scale = 10f64.powi(common::location::APPROX_DECIMALS);
    (a * scale).round() == (b * scale).round()
}

/// An update's coordinates with any component equal to the listing's public
/// point removed, so that component keeps its stored value. Only applies to
/// approximate listings with a stored point. The flag says whether anything
/// was removed.
///
/// A form that loaded the public detail would otherwise save the public point
/// as the stored one, and because the offset is fixed, anyone who saw both
/// public points could subtract to find the real one.
pub fn drop_echoed_public_point(
    latitude: Option<f64>,
    longitude: Option<f64>,
    stored: StoredLocation,
    key: &LocationKey,
    listing_id: Uuid,
) -> (Option<f64>, Option<f64>, bool) {
    let public = match (stored.privacy, stored.point()) {
        (LocationPrivacy::Approximate, Some((lat, lng))) => {
            Some(approximate_point(lat, lng, key, listing_id))
        }
        _ => None,
    };
    match public {
        None => (latitude, longitude, false),
        Some((public_lat, public_lng)) => {
            let lat = latitude.filter(|v| !same_to_five_places(*v, public_lat));
            let lng = longitude.filter(|v| !same_to_five_places(*v, public_lng));
            let echoed = lat != latitude || lng != longitude;
            (lat, lng, echoed)
        }
    }
}
