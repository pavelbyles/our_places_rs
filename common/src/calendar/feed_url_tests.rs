//! Feed URL validation and IP blocklist tests.

use super::feed_url::{FeedUrlError, ip_allowed, ip_allowed_with, redact, validate_feed_url};
use std::net::IpAddr;
use uuid::Uuid;

fn own() -> Uuid {
    Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888)
}

fn ip(s: &str) -> IpAddr {
    s.parse().expect("ip literal")
}

#[test]
fn a_public_https_url_is_accepted() {
    let url = validate_feed_url(
        "  https://www.airbnb.com/calendar/ical/123.ics?s=secret  ",
        own(),
    )
    .expect("valid");
    assert_eq!(url.host_str(), Some("www.airbnb.com"));
}

#[test]
fn http_is_refused() {
    assert_eq!(
        validate_feed_url("http://www.airbnb.com/a.ics", own()),
        Err(FeedUrlError::NotHttps)
    );
}

#[test]
fn user_info_is_refused() {
    for raw in [
        "https://user@host.example/a.ics",
        "https://u:p@host.example/a.ics",
    ] {
        assert_eq!(
            validate_feed_url(raw, own()),
            Err(FeedUrlError::UserInfo),
            "{raw}"
        );
    }
}

#[test]
fn garbage_and_overlong_urls_are_refused() {
    assert_eq!(
        validate_feed_url("not a url", own()),
        Err(FeedUrlError::Malformed)
    );
    assert_eq!(validate_feed_url("", own()), Err(FeedUrlError::Malformed));
    let long = format!("https://host.example/{}", "a".repeat(2048));
    assert_eq!(validate_feed_url(&long, own()), Err(FeedUrlError::TooLong));
}

#[test]
fn our_own_export_url_is_refused_on_any_host() {
    for host in ["guest.ourplaces.example", "evil.example"] {
        let raw = format!("https://{host}/listings/{}/calendar.ics", own());
        assert_eq!(validate_feed_url(&raw, own()), Err(FeedUrlError::SelfFeed));
    }
    let upper = format!(
        "https://evil.example/listings/{}/calendar.ics",
        own().to_string().to_uppercase()
    );
    assert_eq!(
        validate_feed_url(&upper, own()),
        Err(FeedUrlError::SelfFeed)
    );
}

#[test]
fn another_listings_export_url_is_allowed() {
    let raw = format!(
        "https://guest.ourplaces.example/listings/{}/calendar.ics",
        Uuid::from_u128(42)
    );
    assert!(validate_feed_url(&raw, own()).is_ok());
}

#[test]
fn encoded_and_mapped_loopback_literals_are_refused() {
    for raw in [
        "https://127.0.0.1/a.ics",
        "https://2130706433/a.ics",
        "https://0x7f.1/a.ics",
        "https://017700000001/a.ics",
        "https://[::1]/a.ics",
        "https://[::ffff:7f00:1]/a.ics",
        "https://[64:ff9b::7f00:1]/a.ics",
        "https://[2002:7f00:1::]/a.ics",
        "https://169.254.169.254/latest/meta-data",
        "https://10.0.0.5/a.ics",
    ] {
        assert_eq!(
            validate_feed_url(raw, own()),
            Err(FeedUrlError::AddressNotAllowed),
            "{raw}"
        );
    }
}

#[test]
fn every_ipv4_block_is_refused() {
    for s in [
        "0.0.0.0",
        "0.1.2.3",
        "10.1.2.3",
        "100.64.0.1",
        "100.127.255.254",
        "127.0.0.1",
        "169.254.1.1",
        "172.16.0.1",
        "172.31.255.255",
        "192.0.0.8",
        "192.0.2.1",
        "192.168.1.1",
        "198.18.0.1",
        "198.19.255.255",
        "198.51.100.7",
        "203.0.113.9",
        "224.0.0.1",
        "239.255.255.250",
        "240.0.0.1",
        "255.255.255.255",
    ] {
        assert!(!ip_allowed(ip(s)), "{s} should be blocked");
    }
}

#[test]
fn every_ipv6_block_is_refused() {
    for s in [
        "::",
        "::1",
        "::ffff:10.0.0.1",
        "::ffff:127.0.0.1",
        "::8.8.8.8",
        "64:ff9b::8.8.8.8",
        "2002:808:808::1",
        "fc00::1",
        "fd12:3456::1",
        "fe80::1",
        "ff02::1",
        "2001:db8::1",
        "fd00:ec2::254",
    ] {
        assert!(!ip_allowed(ip(s)), "{s} should be blocked");
    }
}

#[test]
fn public_addresses_are_allowed() {
    for s in [
        "8.8.8.8",
        "1.1.1.1",
        "100.63.255.255",
        "100.128.0.0",
        "172.15.255.255",
        "172.32.0.0",
        "2606:4700:4700::1111",
        "::ffff:8.8.8.8",
    ] {
        assert!(ip_allowed(ip(s)), "{s} should be allowed");
    }
}

#[test]
fn allow_private_lets_private_through_but_never_metadata() {
    assert!(ip_allowed_with(ip("127.0.0.1"), true));
    assert!(ip_allowed_with(ip("10.0.0.5"), true));
    assert!(ip_allowed_with(ip("::1"), true));
    assert!(!ip_allowed_with(ip("169.254.169.254"), true));
    assert!(!ip_allowed_with(ip("fd00:ec2::254"), true));
    assert!(!ip_allowed_with(ip("::ffff:169.254.169.254"), true));
    assert!(!ip_allowed_with(ip("127.0.0.1"), false));
}

#[test]
fn redact_keeps_only_the_host() {
    let url = url::Url::parse("https://www.airbnb.com:8443/calendar/ical/123.ics?s=secret#x")
        .expect("url");
    let shown = redact(&url);
    assert_eq!(shown, "www.airbnb.com");
    assert!(!shown.contains("secret") && !shown.contains("ical"));
}

#[test]
fn errors_never_echo_the_url() {
    let raw = "http://secret-token@host.example/path/secret.ics?token=secret";
    let err = validate_feed_url(raw, own()).expect_err("refused");
    let text = format!("{err} {err:?}");
    assert!(!text.contains("secret"), "{text}");
    assert!(!err.code().is_empty());
}
