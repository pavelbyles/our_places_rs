//! Spec 103 §1.10: `MAP_TILES_BASE_URL` is an allowlist.

use super::*;
use std::io::Write;
use std::sync::{Arc, Mutex};

const DEV: &str = "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113";
const EMULATOR: &str = "http://localhost:4443";

/// Captures what `f` logs on this thread.
fn logs_of<T>(f: impl FnOnce() -> T) -> (T, String) {
    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl Write for Buf {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            if let Ok(mut b) = self.0.lock() {
                b.extend_from_slice(data);
            }
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let buf = Buf::default();
    let writer = buf.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let out = tracing::subscriber::with_default(subscriber, f);
    let text = buf
        .0
        .lock()
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    (out, text)
}

#[test]
fn a_tile_folder_is_accepted_with_its_bucket_origin() {
    let config = MapConfig::parse(DEV, None);
    assert_eq!(
        config.as_ref().map(MapConfig::origin),
        Ok("https://storage.googleapis.com/our-places-gcs-map-tiles-dev/")
    );
    assert_eq!(config.as_ref().map(MapConfig::base_url), Ok(DEV));
}

#[test]
fn everything_else_is_refused() {
    let refused = [
        "https://storage.googleapis.com/",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v2026-11",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v2026111",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113/",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113?x=1",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113#x",
        "https://storage.googleapis.com/our-places%2Fx/v20261113",
        "https://storage.googleapis.com/a..b/v20261113",
        "https://storage.googleapis.com/../v20261113",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/../v20261113",
        "https://storage.googleapis.com/our-places\tx/v20261113",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113\r",
        " https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113",
        "https://storage.googleapis.com//v20261113",
        "https://storage.googleapis.com/Our-Places/v20261113",
        "https://storage.googleapis.com/-bucket/v20261113",
        "http://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113",
        "https://storage.googleapis.com.evil.example/our-places/v20261113",
        "https://evil.example/our-places-gcs-map-tiles-dev/v20261113",
        "http://localhost:4443/our-places-gcs-map-tiles-local/v20261113",
    ];
    for raw in refused {
        assert_eq!(
            MapConfig::parse(raw, None),
            Err(MapConfigError::Invalid),
            "{raw:?}"
        );
    }
    assert_eq!(MapConfig::parse("", None), Err(MapConfigError::Unset));
}

#[test]
fn the_emulator_form_needs_the_emulator() {
    let local = "http://localhost:4443/our-places-gcs-map-tiles-local/v20261113";
    assert_eq!(
        MapConfig::parse(local, Some(EMULATOR)).map(|c| c.origin().to_string()),
        Ok("http://localhost:4443/our-places-gcs-map-tiles-local/".to_string())
    );
    assert!(MapConfig::parse(local, None).is_err());
    assert!(MapConfig::parse(local, Some("http://localhost:9999")).is_err());
    // An emulator endpoint that could break a CSP directive is ignored.
    let odd = "http://local;host";
    assert!(MapConfig::parse(&format!("{odd}/our-places-local/v20261113"), Some(odd)).is_err());
    // Production URLs still work with an emulator set.
    assert!(MapConfig::parse(DEV, Some(EMULATOR)).is_ok());
}

#[test]
fn unset_and_invalid_each_warn_once_and_turn_maps_off() {
    let (config, logs) = logs_of(|| MapConfig::from_lookup(|_| None, None));
    assert_eq!(config, None);
    assert_eq!(logs.matches(" WARN ").count(), 1, "{logs}");
    assert!(logs.contains("not set"), "{logs}");

    let (config, logs) = logs_of(|| {
        MapConfig::from_lookup(
            |_| Some("https://storage.googleapis.com/".to_string()),
            None,
        )
    });
    assert_eq!(config, None);
    assert_eq!(logs.matches(" WARN ").count(), 1, "{logs}");
    assert!(logs.contains("not a tile folder URL"), "{logs}");

    let (config, logs) = logs_of(|| MapConfig::from_lookup(|_| Some(DEV.to_string()), None));
    assert!(config.is_some());
    assert!(logs.is_empty(), "{logs}");
}
