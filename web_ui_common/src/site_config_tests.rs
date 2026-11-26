use std::collections::HashMap;

use super::*;

fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |key| map.get(key).cloned()
}

fn url_only(url: &str) -> Result<SiteConfig, SiteConfigError> {
    SiteConfig::from_lookup(lookup(&[(GUEST_PORTAL_URL_ENV, url)]))
}

#[test]
fn invalid_urls_are_rejected_without_echoing_value() {
    assert_eq!(
        SiteConfig::from_lookup(lookup(&[])),
        Err(SiteConfigError::Missing)
    );
    assert_eq!(
        SiteConfig::from_lookup(lookup(&[
            (GUEST_PORTAL_URL_ENV, "http://ourplaces.io"),
            (SEO_INDEXABLE_ENV, "true"),
        ])),
        Err(SiteConfigError::InsecureWhenIndexable)
    );

    for (raw, expected) in [
        ("", SiteConfigError::Missing),
        ("ourplaces.io", SiteConfigError::NotAbsolute),
        (
            "https://secret-host.example/?token=abc",
            SiteConfigError::NotAbsolute,
        ),
        (
            "https://secret-host.example/#top",
            SiteConfigError::NotAbsolute,
        ),
        ("ftp://secret-host.example", SiteConfigError::NotAbsolute),
        ("https://secret-host.example/app", SiteConfigError::HasPath),
    ] {
        let err = url_only(raw).expect_err(raw);
        assert_eq!(err, expected, "{raw:?}");
        let msg = err.to_string();
        assert!(msg.contains(GUEST_PORTAL_URL_ENV), "{msg}");
        if !raw.is_empty() {
            assert!(!msg.contains(raw), "{msg}");
            assert!(!msg.contains("secret-host"), "{msg}");
        }
    }
}

#[test]
fn valid_urls_are_accepted_and_normalised() {
    for (raw, expected_url) in [
        ("http://localhost:3010", "http://localhost:3010"),
        ("https://ourplaces.io/", "https://ourplaces.io"),
    ] {
        let config = url_only(raw).expect(raw);
        assert_eq!(config.base_url().as_str(), expected_url, "{raw}");
        assert!(!config.indexable(), "{raw}");
    }
}

#[test]
fn indexable_flag_accepts_only_true_and_one() {
    assert_eq!(
        url_only("https://ourplaces.io").map(|c| c.indexable()),
        Ok(false)
    );
    for (raw, expected) in [
        ("true", true),
        ("1", true),
        ("TRUE ", false),
        ("True", false),
        ("yes", false),
        ("0", false),
        ("false", false),
        ("", false),
    ] {
        let config = SiteConfig::from_lookup(lookup(&[
            (GUEST_PORTAL_URL_ENV, "https://ourplaces.io"),
            (SEO_INDEXABLE_ENV, raw),
        ]));
        assert_eq!(config.map(|c| c.indexable()), Ok(expected), "{raw:?}");
    }
}

#[test]
fn twitter_handle_and_page_title() {
    assert_eq!(
        url_only("https://ourplaces.io")
            .as_ref()
            .map(|c| c.twitter_site()),
        Ok(None)
    );

    for (raw, expected) in [
        ("@our_places1", Some("@our_places1")),
        ("ourplaces", None),
        ("@", None),
        ("@sixteen_chars_xx", None),
        ("@bad-char", None),
        ("@a b", None),
        ("", None),
    ] {
        let config = SiteConfig::from_lookup(lookup(&[
            (GUEST_PORTAL_URL_ENV, "https://ourplaces.io"),
            (SEO_TWITTER_SITE_ENV, raw),
        ]));
        assert_eq!(
            config.as_ref().map(|c| c.twitter_site()),
            Ok(expected),
            "{raw:?}"
        );
    }

    assert_eq!(
        page_title("Villas in Jamaica"),
        "Villas in Jamaica · Our Places"
    );
}
