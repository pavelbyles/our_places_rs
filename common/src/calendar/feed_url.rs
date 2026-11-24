//! Rules for imported feed URLs: https only, no user info, never our own
//! export, and never an address inside a private or reserved range.
//!
//! Feed URLs carry a read token, so nothing here ever puts a URL, path or
//! query into an error or a log line. Use [`redact`] to name a feed's host.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use thiserror::Error;
use url::{Host, Url};
use uuid::Uuid;

/// Longest feed URL accepted.
pub const MAX_FEED_URL_LEN: usize = 2048;

/// Why a feed URL was refused. `Display` never contains the URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FeedUrlError {
    /// The URL couldn't be parsed.
    #[error("the calendar URL is not a valid URL")]
    Malformed,
    /// The URL is longer than [`MAX_FEED_URL_LEN`].
    #[error("the calendar URL is too long")]
    TooLong,
    /// The URL isn't `https`.
    #[error("the calendar URL must use https")]
    NotHttps,
    /// The URL has no host.
    #[error("the calendar URL has no host")]
    MissingHost,
    /// The URL contains a user name or password.
    #[error("the calendar URL must not contain a user name or password")]
    UserInfo,
    /// The host is an IP address in a private or reserved range.
    #[error("address not allowed")]
    AddressNotAllowed,
    /// The URL is this listing's own export feed.
    #[error("this is the listing's own export calendar")]
    SelfFeed,
}

impl FeedUrlError {
    /// Stable API error code.
    pub fn code(&self) -> &'static str {
        match self {
            FeedUrlError::SelfFeed => "CALENDAR_FEED_SELF",
            _ => "CALENDAR_FEED_INVALID_URL",
        }
    }
}

/// Checks a feed URL a manager entered for `own_listing_id`, and returns it
/// parsed. Hostnames are checked again when they're resolved at fetch time.
pub fn validate_feed_url(raw: &str, own_listing_id: Uuid) -> Result<Url, FeedUrlError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(FeedUrlError::Malformed);
    }
    if raw.len() > MAX_FEED_URL_LEN {
        return Err(FeedUrlError::TooLong);
    }
    let url = Url::parse(raw).map_err(|_| FeedUrlError::Malformed)?;
    if url.scheme() != "https" {
        return Err(FeedUrlError::NotHttps);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FeedUrlError::UserInfo);
    }
    match url.host() {
        None => return Err(FeedUrlError::MissingHost),
        Some(host) if !host_allowed(&host, false) => {
            return Err(FeedUrlError::AddressNotAllowed);
        }
        Some(_) => {}
    }
    if is_own_export(&url, own_listing_id) {
        return Err(FeedUrlError::SelfFeed);
    }
    Ok(url)
}

/// Whether a parsed host may be contacted: IP literals are checked against
/// the blocklist, names are left to the resolver.
pub fn host_allowed(host: &Host<&str>, allow_private: bool) -> bool {
    match host {
        Host::Ipv4(v4) => ip_allowed_with(IpAddr::V4(*v4), allow_private),
        Host::Ipv6(v6) => ip_allowed_with(IpAddr::V6(*v6), allow_private),
        Host::Domain(_) => true,
    }
}

/// Whether the path is `/listings/{own_listing_id}/calendar.ics`.
fn is_own_export(url: &Url, own_listing_id: Uuid) -> bool {
    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|seg| !seg.is_empty()).collect())
        .unwrap_or_default();
    matches!(
        segments.as_slice(),
        [listings, id, file]
            if listings.eq_ignore_ascii_case("listings")
                && file.eq_ignore_ascii_case("calendar.ics")
                && Uuid::parse_str(id).is_ok_and(|id| id == own_listing_id)
    )
}

/// Whether a feed fetch may connect to `ip` under the production rules.
pub fn ip_allowed(ip: IpAddr) -> bool {
    ip_allowed_with(ip, false)
}

/// Like [`ip_allowed`], but `allow_private` (local development only) lets
/// private and loopback ranges through. Metadata addresses stay blocked.
pub fn ip_allowed_with(ip: IpAddr, allow_private: bool) -> bool {
    if is_metadata(ip) {
        return false;
    }
    allow_private || !is_blocked(ip)
}

/// Cloud metadata endpoints, blocked in every configuration.
fn is_metadata(ip: IpAddr) -> bool {
    const AWS_V6: Ipv6Addr = Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254);
    match ip {
        IpAddr::V4(v4) => v4 == Ipv4Addr::new(169, 254, 169, 254),
        IpAddr::V6(v6) => {
            v6 == AWS_V6 || v6.to_ipv4_mapped().is_some_and(|v4| is_metadata(v4.into()))
        }
    }
}

fn is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_blocked(v4),
        IpAddr::V6(v6) => v6_blocked(v6),
    }
}

/// `(network, prefix length)` pairs blocked for IPv4.
const V4_BLOCKS: &[([u8; 4], u32)] = &[
    ([0, 0, 0, 0], 8),
    ([10, 0, 0, 0], 8),
    ([100, 64, 0, 0], 10),
    ([127, 0, 0, 0], 8),
    ([169, 254, 0, 0], 16),
    ([172, 16, 0, 0], 12),
    ([192, 0, 0, 0], 24),
    ([192, 0, 2, 0], 24),
    ([192, 168, 0, 0], 16),
    ([198, 18, 0, 0], 15),
    ([198, 51, 100, 0], 24),
    ([203, 0, 113, 0], 24),
    ([224, 0, 0, 0], 4),
    ([240, 0, 0, 0], 4),
];

fn v4_blocked(ip: Ipv4Addr) -> bool {
    let bits = u32::from(ip);
    ip.is_broadcast()
        || V4_BLOCKS.iter().any(|(net, prefix)| {
            let mask = u32::MAX.checked_shl(32 - prefix).unwrap_or(0);
            bits & mask == u32::from_be_bytes(*net) & mask
        })
}

/// `(network, prefix length)` pairs blocked for IPv6, besides `::/128`,
/// `::1` and mapped addresses, which are handled separately.
const V6_BLOCKS: &[(u128, u32)] = &[
    (0, 96),                                         // ::/96 IPv4-compatible
    (0x0064_ff9b_0000_0000_0000_0000_0000_0000, 96), // NAT64
    (0x2002_0000_0000_0000_0000_0000_0000_0000, 16), // 6to4
    (0xfc00_0000_0000_0000_0000_0000_0000_0000, 7),  // unique local
    (0xfe80_0000_0000_0000_0000_0000_0000_0000, 10), // link local
    (0xff00_0000_0000_0000_0000_0000_0000_0000, 8),  // multicast
    (0x2001_0db8_0000_0000_0000_0000_0000_0000, 32), // documentation
];

fn v6_blocked(ip: Ipv6Addr) -> bool {
    if ip.is_unspecified() || ip.is_loopback() {
        return true;
    }
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_blocked(v4);
    }
    let bits = u128::from(ip);
    V6_BLOCKS.iter().any(|(net, prefix)| {
        let mask = u128::MAX.checked_shl(128 - prefix).unwrap_or(0);
        bits & mask == net & mask
    })
}

/// A feed URL's host, for logs and spans. Never the path or query.
pub fn redact(url: &Url) -> String {
    url.host_str().unwrap_or("<no host>").to_string()
}
