//! An SSRF-guarded HTTP client for imported calendar feeds (spec §1.4.1, §3.4).
//!
//! - `https` only, on the first URL and on every redirect.
//! - IP literals are checked against the blocklist directly, because hyper
//!   connects to them without calling the resolver.
//! - Hostnames resolve through [`GuardedResolver`], which drops blocked
//!   addresses at connect time, for redirects too. That closes DNS rebinding.
//! - No proxy, at most 3 redirects, a streamed 2 MB cap, and timeouts.
//!
//! Feed URLs carry a read token. Every `reqwest::Error` goes through
//! `.without_url()`, and [`FetchError`] never holds a URL, path or query.

use common::calendar::feed_url::{host_allowed, ip_allowed_with};
use common::calendar::ics_parse::MAX_FEED_BYTES;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use reqwest::{Certificate, redirect};
use std::error::Error as StdError;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use url::Url;

/// `User-Agent` sent with every feed fetch.
pub const FEED_USER_AGENT: &str = "OurPlaces-CalendarSync/1.0";
/// `Accept` sent with every feed fetch.
pub const FEED_ACCEPT: &str = "text/calendar, */*;q=0.1";

/// Why a feed couldn't be fetched. Never contains a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FetchError {
    /// The URL isn't `https` or has no host.
    #[error("calendar URL is not allowed")]
    InvalidUrl,
    /// The host is, or resolves only to, a private or reserved address.
    #[error("address not allowed")]
    AddressNotAllowed,
    /// A redirect was refused: too many, not `https`, or to a blocked address.
    #[error("redirect not allowed")]
    Redirect,
    /// Connecting or the whole fetch took too long.
    #[error("calendar server timed out")]
    Timeout,
    /// The connection or TLS handshake failed.
    #[error("could not connect to the calendar server")]
    Connect,
    /// The server answered with an HTTP error status.
    #[error("calendar server returned HTTP {0}")]
    HttpStatus(u16),
    /// The body is larger than the cap.
    #[error("feed is larger than 2 MB")]
    TooLarge,
    /// Reading the body failed.
    #[error("could not read the calendar feed")]
    Body,
    /// Any other request failure.
    #[error("calendar request failed")]
    Request,
    /// The HTTP client couldn't be built.
    #[error("calendar client could not be configured")]
    Client,
}

impl FetchError {
    /// Stable code.
    pub fn code(&self) -> &'static str {
        match self {
            FetchError::InvalidUrl => "fetch_invalid_url",
            FetchError::AddressNotAllowed => "address_not_allowed",
            FetchError::Redirect => "redirect_not_allowed",
            FetchError::Timeout => "fetch_timeout",
            FetchError::Connect => "fetch_connect_failed",
            FetchError::HttpStatus(_) => "fetch_http_status",
            FetchError::TooLarge => "feed_too_large",
            FetchError::Body => "fetch_body_failed",
            FetchError::Request => "fetch_failed",
            FetchError::Client => "fetch_client_failed",
        }
    }
}

/// Fetcher limits and test hooks. [`Default`] is the production setup.
#[derive(Clone)]
pub struct FetcherConfig {
    /// Time allowed to connect.
    pub connect_timeout: Duration,
    /// Time allowed for the whole fetch, body included.
    pub total_timeout: Duration,
    /// Largest body accepted.
    pub max_bytes: usize,
    /// Most redirects followed.
    pub max_redirects: usize,
    /// Lets private and loopback addresses through (local development only).
    /// Metadata addresses stay blocked.
    pub allow_private_hosts: bool,
    /// An extra trusted root, for local fixture servers.
    pub extra_root_ca: Option<Certificate>,
    /// Replaces system DNS. Tests only; always wrapped by the address filter.
    pub resolver: Option<Arc<dyn Resolve>>,
}

impl Default for FetcherConfig {
    fn default() -> Self {
        FetcherConfig {
            connect_timeout: Duration::from_secs(5),
            total_timeout: Duration::from_secs(10),
            max_bytes: MAX_FEED_BYTES,
            max_redirects: 3,
            allow_private_hosts: false,
            extra_root_ca: None,
            resolver: None,
        }
    }
}

impl fmt::Debug for FetcherConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FetcherConfig")
            .field("connect_timeout", &self.connect_timeout)
            .field("total_timeout", &self.total_timeout)
            .field("max_bytes", &self.max_bytes)
            .field("max_redirects", &self.max_redirects)
            .field("allow_private_hosts", &self.allow_private_hosts)
            .field("extra_root_ca", &self.extra_root_ca.is_some())
            .field("resolver", &self.resolver.is_some())
            .finish()
    }
}

/// Marker error: a host resolved only to blocked addresses.
#[derive(Debug)]
struct BlockedAddress;

impl fmt::Display for BlockedAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("address not allowed")
    }
}

impl StdError for BlockedAddress {}

/// Marker error: the redirect policy refused a hop.
#[derive(Debug)]
struct RedirectRefused;

impl fmt::Display for RedirectRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("redirect not allowed")
    }
}

impl StdError for RedirectRefused {}

/// Resolves names (through system DNS or an injected resolver) and drops
/// every address the blocklist refuses.
struct GuardedResolver {
    inner: Option<Arc<dyn Resolve>>,
    allow_private: bool,
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let inner = self.inner.clone();
        let allow_private = self.allow_private;
        Box::pin(async move {
            let found: Vec<SocketAddr> = match inner {
                Some(resolver) => resolver.resolve(name).await?.collect(),
                None => tokio::net::lookup_host((name.as_str(), 0)).await?.collect(),
            };
            let allowed: Vec<SocketAddr> = found
                .into_iter()
                .filter(|addr| ip_allowed_with(addr.ip(), allow_private))
                .collect();
            match allowed.is_empty() {
                true => Err(Box::new(BlockedAddress) as Box<dyn StdError + Send + Sync>),
                false => Ok(Box::new(allowed.into_iter()) as Addrs),
            }
        })
    }
}

/// Fetches feed bodies under the SSRF rules above.
#[derive(Clone)]
pub struct GuardedFetcher {
    client: reqwest::Client,
    max_bytes: usize,
    total_timeout: Duration,
    allow_private: bool,
}

impl fmt::Debug for GuardedFetcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GuardedFetcher")
            .field("max_bytes", &self.max_bytes)
            .field("allow_private", &self.allow_private)
            .finish_non_exhaustive()
    }
}

impl GuardedFetcher {
    /// Builds the client.
    pub fn new(config: FetcherConfig) -> Result<Self, FetchError> {
        let allow_private = config.allow_private_hosts;
        let max_redirects = config.max_redirects;
        let policy = redirect::Policy::custom(move |attempt| {
            let allowed = attempt.previous().len() <= max_redirects
                && attempt.url().scheme() == "https"
                && attempt
                    .url()
                    .host()
                    .is_some_and(|host| host_allowed(&host, allow_private));
            match allowed {
                true => attempt.follow(),
                false => attempt.error(RedirectRefused),
            }
        });
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static(FEED_ACCEPT));
        headers.insert(USER_AGENT, HeaderValue::from_static(FEED_USER_AGENT));
        let builder = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .redirect(policy)
            .connect_timeout(config.connect_timeout)
            .timeout(config.total_timeout)
            .default_headers(headers)
            .dns_resolver(Arc::new(GuardedResolver {
                inner: config.resolver,
                allow_private,
            }));
        let builder = match config.extra_root_ca {
            Some(ca) => builder.tls_certs_merge([ca]),
            None => builder,
        };
        let client = builder.build().map_err(|e| {
            tracing::error!(error = %e.without_url(), "calendar fetch client build failed");
            FetchError::Client
        })?;
        Ok(GuardedFetcher {
            client,
            max_bytes: config.max_bytes,
            total_timeout: config.total_timeout,
            allow_private,
        })
    }

    /// Fetches `url`'s body, at most `max_bytes` long.
    pub async fn fetch(&self, url: &Url) -> Result<Vec<u8>, FetchError> {
        if url.scheme() != "https" {
            return Err(FetchError::InvalidUrl);
        }
        match url.host() {
            None => return Err(FetchError::InvalidUrl),
            Some(host) if !host_allowed(&host, self.allow_private) => {
                return Err(FetchError::AddressNotAllowed);
            }
            Some(_) => {}
        }
        tokio::time::timeout(self.total_timeout, self.fetch_body(url.clone()))
            .await
            .unwrap_or(Err(FetchError::Timeout))
    }

    async fn fetch_body(&self, url: Url) -> Result<Vec<u8>, FetchError> {
        let mut response = self.client.get(url).send().await.map_err(map_reqwest)?;
        let status = response.status();
        if status.is_client_error() || status.is_server_error() {
            return Err(FetchError::HttpStatus(status.as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|len| len > self.max_bytes as u64)
        {
            return Err(FetchError::TooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(map_reqwest)? {
            if body.len() + chunk.len() > self.max_bytes {
                return Err(FetchError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

/// Whether `e` or any of its causes is a `T`.
fn caused_by<T: StdError + 'static>(e: &(dyn StdError + 'static)) -> bool {
    let mut current = Some(e);
    while let Some(err) = current {
        if err.is::<T>() {
            return true;
        }
        current = err.source();
    }
    false
}

fn map_reqwest(e: reqwest::Error) -> FetchError {
    let e = e.without_url();
    match () {
        _ if caused_by::<BlockedAddress>(&e) => FetchError::AddressNotAllowed,
        _ if caused_by::<RedirectRefused>(&e) || e.is_redirect() => FetchError::Redirect,
        _ if e.is_timeout() => FetchError::Timeout,
        _ if e.is_connect() => FetchError::Connect,
        _ if e.is_body() || e.is_decode() => FetchError::Body,
        _ => FetchError::Request,
    }
}
