use super::*;

const KEY: &str = "test-location-offset-key-0123456789abcdef";
const OTHER_KEY: &str = "another-location-offset-key-0123456789ab";

fn key(raw: &str) -> LocationKey {
    match LocationKey::from_secret(raw) {
        Ok(key) => key,
        Err(e) => panic!("test key refused: {e}"),
    }
}

/// Great-circle distance in metres.
fn haversine(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let d_lat = lat2 - lat1;
    let d_lng = (b.1 - a.1).to_radians();
    let h = (d_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lng / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// Whether `value` has at most `APPROX_DECIMALS` decimal places.
fn has_at_most_five_places(value: f64) -> bool {
    let scaled = value * 1e5;
    (scaled - scaled.round()).abs() < 1e-6
}

/// Deterministic, well-spread listing IDs.
fn ids(n: u128) -> impl Iterator<Item = Uuid> {
    (1..=n).map(|i| Uuid::from_u128(i.wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835)))
}

const OCHO_RIOS: (f64, f64) = (18.4074, -77.1031);

#[test]
fn the_same_key_and_listing_give_the_same_point() {
    let k = key(KEY);
    let id = Uuid::from_u128(42);
    let first = approximate_point(OCHO_RIOS.0, OCHO_RIOS.1, &k, id);
    let second = approximate_point(OCHO_RIOS.0, OCHO_RIOS.1, &key(KEY), id);
    assert_eq!(first, second);
}

#[test]
fn a_different_key_or_listing_gives_a_different_point() {
    let id = Uuid::from_u128(42);
    let base = approximate_point(OCHO_RIOS.0, OCHO_RIOS.1, &key(KEY), id);
    let other_key = approximate_point(OCHO_RIOS.0, OCHO_RIOS.1, &key(OTHER_KEY), id);
    let other_id = approximate_point(OCHO_RIOS.0, OCHO_RIOS.1, &key(KEY), Uuid::from_u128(43));
    assert_ne!(base, other_key);
    assert_ne!(base, other_id);
}

#[test]
fn every_offset_is_between_150_and_500_metres() {
    let k = key(KEY);
    for id in ids(10_000) {
        let point = approximate_point(OCHO_RIOS.0, OCHO_RIOS.1, &k, id);
        let distance = haversine(OCHO_RIOS, point);
        // Rounding to 5 places moves a point by at most about 0.8 m.
        assert!(
            (APPROX_MIN_M - 1.0..=APPROX_MAX_M + 1.5).contains(&distance),
            "{id}: {distance} m"
        );
        assert!(has_at_most_five_places(point.0), "{id}: lat {}", point.0);
        assert!(has_at_most_five_places(point.1), "{id}: lng {}", point.1);
    }
}

#[test]
fn offsets_cover_every_direction() {
    let k = key(KEY);
    let mut quadrants = [false; 4];
    for id in ids(200) {
        let (lat, lng) = approximate_point(OCHO_RIOS.0, OCHO_RIOS.1, &k, id);
        let north = lat > OCHO_RIOS.0;
        let east = lng > OCHO_RIOS.1;
        quadrants[usize::from(north) * 2 + usize::from(east)] = true;
    }
    assert_eq!(quadrants, [true; 4]);
}

#[test]
fn the_longitude_offset_is_scaled_for_latitude() {
    // A purely east-west offset of d metres spans more degrees of longitude
    // than of latitude at 18°N; the haversine check above only holds if the
    // scaling is applied, so compare the spread of degrees directly.
    let k = key(KEY);
    let (max_dlat, max_dlng) = ids(2_000).fold((0f64, 0f64), |(mlat, mlng), id| {
        let (lat, lng) = approximate_point(OCHO_RIOS.0, OCHO_RIOS.1, &k, id);
        (
            mlat.max((lat - OCHO_RIOS.0).abs()),
            mlng.max((lng - OCHO_RIOS.1).abs()),
        )
    });
    let ratio = max_dlng / max_dlat;
    let expected = 1.0 / OCHO_RIOS.0.to_radians().cos();
    assert!(
        (ratio - expected).abs() < 0.02,
        "ratio {ratio}, expected {expected}"
    );
}

#[test]
fn latitude_is_clamped_at_the_poles() {
    let k = key(KEY);
    for id in ids(500) {
        let (lat, lng) = approximate_point(89.9999, 10.0, &k, id);
        assert!((-90.0..=90.0).contains(&lat), "{lat}");
        assert!((-180.0..180.0).contains(&lng), "{lng}");
    }
}

#[test]
fn longitude_wraps_after_rounding() {
    assert_eq!(
        round_coordinate(wrap_longitude(round_coordinate(179.999996))),
        -180.0
    );
    assert_eq!(
        round_coordinate(wrap_longitude(round_coordinate(-180.000004))),
        -180.0
    );
    let k = key(KEY);
    for id in ids(500) {
        let (_, lng) = approximate_point(18.0, 179.9999, &k, id);
        assert!((-180.0..180.0).contains(&lng), "{lng}");
        assert!(has_at_most_five_places(lng), "{lng}");
    }
}

#[test]
fn short_or_blank_keys_are_refused() {
    assert_eq!(
        LocationKey::from_secret("").map(|_| ()),
        Err(LocationKeyError::Missing)
    );
    assert_eq!(
        LocationKey::from_secret("   \n").map(|_| ()),
        Err(LocationKeyError::Missing)
    );
    let thirty_one = "a".repeat(MIN_KEY_BYTES - 1);
    assert_eq!(
        LocationKey::from_secret(&thirty_one).map(|_| ()),
        Err(LocationKeyError::TooShort)
    );
    // Whitespace doesn't count towards the length.
    let padded = format!("  {thirty_one}  ");
    assert_eq!(
        LocationKey::from_secret(&padded).map(|_| ()),
        Err(LocationKeyError::TooShort)
    );
    assert!(LocationKey::from_secret(&"a".repeat(MIN_KEY_BYTES)).is_ok());
}

#[test]
fn errors_and_debug_never_show_the_key() {
    let k = key(KEY);
    let debug = format!("{k:?}");
    assert!(!debug.contains(KEY), "{debug}");
    assert_eq!(debug, "LocationKey(**redacted**)");
    let short = "short-but-secret";
    let message = match LocationKey::from_secret(short) {
        Ok(_) => panic!("a short key was accepted"),
        Err(e) => e.to_string(),
    };
    assert!(!message.contains(short), "{message}");
}

#[test]
fn privacy_round_trips_and_defaults_to_approximate() {
    assert_eq!(LocationPrivacy::default(), LocationPrivacy::Approximate);
    for privacy in [LocationPrivacy::Approximate, LocationPrivacy::Exact] {
        let json = serde_json::to_string(&privacy).unwrap_or_default();
        assert_eq!(json, format!("\"{}\"", privacy.as_str()));
        let back: Result<LocationPrivacy, _> = serde_json::from_str(&json);
        assert_eq!(back.ok(), Some(privacy));
    }
    let unknown: Result<LocationPrivacy, _> = serde_json::from_str("\"nearby\"");
    assert!(unknown.is_err());
    assert_eq!(
        LocationPrecision::from(LocationPrivacy::Exact),
        LocationPrecision::Exact
    );
}

#[test]
fn bounds_include_their_edges_and_refuse_non_finite_values() {
    let b = JAMAICA_BOUNDS;
    assert!(in_bounds(b.south, b.west));
    assert!(in_bounds(b.north, b.east));
    assert!(in_bounds(OCHO_RIOS.0, OCHO_RIOS.1));
    assert!(!in_bounds(b.south - 0.00001, b.west));
    assert!(!in_bounds(b.north, b.east + 0.00001));
    assert!(!in_bounds(f64::NAN, OCHO_RIOS.1));
    assert!(!in_bounds(OCHO_RIOS.0, f64::INFINITY));
    // Kingston (in), Miami (out).
    assert!(in_bounds(17.9714, -76.7931));
    assert!(!in_bounds(25.7617, -80.1918));
}
