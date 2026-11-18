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
fn missing_url_is_rejected() {
    assert_eq!(
        SiteConfig::from_lookup(lookup(&[])),
        Err(SiteConfigError::Missing)
    );
}

#[test]
fn empty_url_is_rejected() {
    assert_eq!(url_only(""), Err(SiteConfigError::Missing));
}

#[test]
fn url_without_scheme_is_rejected() {
    assert_eq!(url_only("ourplaces.io"), Err(SiteConfigError::NotAbsolute));
}

#[test]
fn url_with_query_is_rejected() {
    assert_eq!(
        url_only("https://ourplaces.io/?a=1"),
        Err(SiteConfigError::NotAbsolute)
    );
}

#[test]
fn url_with_fragment_is_rejected() {
    assert_eq!(
        url_only("https://ourplaces.io/#top"),
        Err(SiteConfigError::NotAbsolute)
    );
}

#[test]
fn non_http_scheme_is_rejected() {
    assert_eq!(
        url_only("ftp://ourplaces.io"),
        Err(SiteConfigError::NotAbsolute)
    );
}

#[test]
fn url_with_path_is_rejected() {
    assert_eq!(
        url_only("https://ourplaces.io/app"),
        Err(SiteConfigError::HasPath)
    );
}

#[test]
fn local_http_url_is_accepted_when_not_indexable() {
    let config = url_only("http://localhost:3010");
    assert_eq!(
        config.as_ref().map(|c| c.base_url().as_str()),
        Ok("http://localhost:3010")
    );
    assert_eq!(config.map(|c| c.indexable()), Ok(false));
}

#[test]
fn trailing_slash_is_normalised() {
    assert_eq!(
        url_only("https://ourplaces.io/").map(|c| c.base_url().as_str().to_string()),
        Ok("https://ourplaces.io".to_string())
    );
}

#[test]
fn http_url_is_rejected_when_indexable() {
    let config = SiteConfig::from_lookup(lookup(&[
        (GUEST_PORTAL_URL_ENV, "http://ourplaces.io"),
        (SEO_INDEXABLE_ENV, "true"),
    ]));
    assert_eq!(config, Err(SiteConfigError::InsecureWhenIndexable));
}

#[test]
fn indexable_accepts_only_true_and_one() {
    for raw in ["true", "1"] {
        let config = SiteConfig::from_lookup(lookup(&[
            (GUEST_PORTAL_URL_ENV, "https://ourplaces.io"),
            (SEO_INDEXABLE_ENV, raw),
        ]));
        assert_eq!(config.map(|c| c.indexable()), Ok(true), "{raw}");
    }
}

#[test]
fn indexable_is_off_for_anything_else() {
    for raw in ["TRUE ", "True", "yes", "0", "false", ""] {
        let config = SiteConfig::from_lookup(lookup(&[
            (GUEST_PORTAL_URL_ENV, "https://ourplaces.io"),
            (SEO_INDEXABLE_ENV, raw),
        ]));
        assert_eq!(config.map(|c| c.indexable()), Ok(false), "{raw:?}");
    }
}

#[test]
fn indexable_is_off_when_unset() {
    assert_eq!(
        url_only("https://ourplaces.io").map(|c| c.indexable()),
        Ok(false)
    );
}

#[test]
fn valid_twitter_handle_is_kept() {
    let config = SiteConfig::from_lookup(lookup(&[
        (GUEST_PORTAL_URL_ENV, "https://ourplaces.io"),
        (SEO_TWITTER_SITE_ENV, "@our_places1"),
    ]));
    assert_eq!(
        config.map(|c| c.twitter_site().map(str::to_string)),
        Ok(Some("@our_places1".to_string()))
    );
}

#[test]
fn invalid_twitter_handle_is_dropped() {
    for raw in [
        "ourplaces",
        "@",
        "@sixteen_chars_xx",
        "@bad-char",
        "@a b",
        "",
    ] {
        let config = SiteConfig::from_lookup(lookup(&[
            (GUEST_PORTAL_URL_ENV, "https://ourplaces.io"),
            (SEO_TWITTER_SITE_ENV, raw),
        ]));
        assert_eq!(
            config.as_ref().map(|c| c.twitter_site()),
            Ok(None),
            "{raw:?}"
        );
    }
}

#[test]
fn twitter_handle_is_optional() {
    assert_eq!(
        url_only("https://ourplaces.io")
            .as_ref()
            .map(|c| c.twitter_site()),
        Ok(None)
    );
}

#[test]
fn errors_never_echo_the_configured_value() {
    for raw in [
        "ourplaces.io",
        "https://secret-host.example/?token=abc",
        "https://secret-host.example/app",
        "ftp://secret-host.example",
    ] {
        let message = url_only(raw)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(!message.is_empty(), "{raw} should be rejected");
        assert!(!message.contains(raw), "{message}");
        assert!(!message.contains("secret-host"), "{message}");
        assert!(message.contains(GUEST_PORTAL_URL_ENV), "{message}");
    }
}

#[test]
fn page_title_appends_the_site_name() {
    assert_eq!(
        page_title("Villas in Jamaica"),
        "Villas in Jamaica · Our Places"
    );
}
