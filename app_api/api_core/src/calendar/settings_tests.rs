//! Calendar sync settings and the Cloud Run startup guard. Uses injected env
//! lookups, never `set_var`.

use crate::settings::{
    CALENDAR_ALLOW_PRIVATE_HOSTS_ENV, CALENDAR_EXTRA_ROOT_CA_PATH_ENV,
    CALENDAR_JOB_CONCURRENCY_ENV, CLOUD_RUN_JOB_ENV, CalendarSettingsError, CalendarSyncSettings,
    K_SERVICE_ENV, check_cloud_guard,
};
use std::collections::HashMap;

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k| map.get(k).cloned()
}

#[test]
fn defaults_are_the_production_values() {
    let s = CalendarSyncSettings::from_env(env(&[])).expect("settings");
    assert_eq!(s, CalendarSyncSettings::default());
    assert!(!s.allow_private_hosts);
    assert_eq!(s.job_concurrency, 8);
    assert_eq!(s.extra_root_ca_path, None);
}

#[test]
fn values_are_read_from_the_environment() {
    let s = CalendarSyncSettings::from_env(env(&[
        (CALENDAR_ALLOW_PRIVATE_HOSTS_ENV, "true"),
        (CALENDAR_JOB_CONCURRENCY_ENV, "3"),
        (CALENDAR_EXTRA_ROOT_CA_PATH_ENV, "/state/ca.pem"),
    ]))
    .expect("settings");
    assert!(s.allow_private_hosts);
    assert_eq!(s.job_concurrency, 3);
    assert_eq!(
        s.extra_root_ca_path.as_deref(),
        Some(std::path::Path::new("/state/ca.pem"))
    );
}

#[test]
fn bad_values_are_refused() {
    for (k, v) in [
        (CALENDAR_ALLOW_PRIVATE_HOSTS_ENV, "maybe"),
        (CALENDAR_JOB_CONCURRENCY_ENV, "0"),
        (CALENDAR_JOB_CONCURRENCY_ENV, "lots"),
    ] {
        assert_eq!(
            CalendarSyncSettings::from_env(env(&[(k, v)])),
            Err(CalendarSettingsError::Invalid(k))
        );
    }
}

#[test]
fn the_guard_refuses_each_toggle_on_cloud_run() {
    let private = CalendarSyncSettings {
        allow_private_hosts: true,
        ..CalendarSyncSettings::default()
    };
    let ca = CalendarSyncSettings {
        extra_root_ca_path: Some("/ca.pem".into()),
        ..CalendarSyncSettings::default()
    };
    for marker in [K_SERVICE_ENV, CLOUD_RUN_JOB_ENV] {
        assert_eq!(
            check_cloud_guard(&private, env(&[(marker, "listing-api")])),
            Err(CalendarSettingsError::LocalOnlyOnCloud(
                CALENDAR_ALLOW_PRIVATE_HOSTS_ENV
            )),
            "{marker}"
        );
        assert_eq!(
            check_cloud_guard(&ca, env(&[(marker, "calendar-sync")])),
            Err(CalendarSettingsError::LocalOnlyOnCloud(
                CALENDAR_EXTRA_ROOT_CA_PATH_ENV
            )),
            "{marker}"
        );
        assert_eq!(
            check_cloud_guard(&CalendarSyncSettings::default(), env(&[(marker, "x")])),
            Ok(())
        );
    }
}

#[test]
fn the_guard_allows_toggles_locally() {
    let s = CalendarSyncSettings {
        allow_private_hosts: true,
        extra_root_ca_path: Some("/ca.pem".into()),
        ..CalendarSyncSettings::default()
    };
    assert_eq!(check_cloud_guard(&s, env(&[])), Ok(()));
}
