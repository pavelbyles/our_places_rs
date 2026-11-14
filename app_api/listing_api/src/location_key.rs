//! The location offset key (spec 103 §1.4): read once at startup, refused if
//! missing or short, and never logged.

use common::location::{LocationKey, LocationKeyError};

/// Environment variable holding the location offset key.
pub static LOCATION_OFFSET_KEY_ENV: &str = "LOCATION_OFFSET_KEY";

/// Why the configured key can't be used. Names the variable, never its value.
#[derive(Debug, thiserror::Error)]
#[error("{LOCATION_OFFSET_KEY_ENV} is not usable: {reason}")]
pub struct LocationKeyConfigError {
    /// What was wrong with it.
    pub reason: LocationKeyError,
}

/// Reads the key through `get_env`, so tests needn't touch the process
/// environment.
pub fn load(
    get_env: impl Fn(&str) -> Option<String>,
) -> Result<LocationKey, LocationKeyConfigError> {
    let raw = get_env(LOCATION_OFFSET_KEY_ENV).unwrap_or_default();
    LocationKey::from_secret(&raw).map_err(|reason| LocationKeyConfigError { reason })
}

/// A fixed key for test apps.
#[cfg(test)]
pub(crate) const TEST_KEY: &str = "listing-api-test-location-offset-key-0001";

/// The fixed test key, ready for `App::app_data`.
#[cfg(test)]
pub(crate) fn test_key_data() -> actix_web::web::Data<LocationKey> {
    match LocationKey::from_secret(TEST_KEY) {
        Ok(key) => actix_web::web::Data::new(key),
        Err(e) => panic!("test key refused: {e}"),
    }
}

#[cfg(test)]
#[path = "location_key_tests.rs"]
mod location_key_tests;
