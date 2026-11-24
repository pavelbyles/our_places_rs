//! SSRF-guarded fetcher tests against a local https server (spec §3.4).

use super::fetch::{FEED_ACCEPT, FEED_USER_AGENT, FetchError, FetcherConfig, GuardedFetcher};
use super::test_server::{FixedResolver, Reply, Router, TestServer, start};
use std::sync::Arc;
use std::time::Duration;

const ICS: &[u8] = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n";

fn router(port_hint: Arc<std::sync::OnceLock<u16>>) -> Router {
    Arc::new(move |path: &str| {
        let port = port_hint.get().copied().unwrap_or(0);
        match path {
            "/ok.ics" | "/ok.ics?s=secret-token" => Reply::Ok(ICS.to_vec()),
            "/503" => Reply::Status(503),
            "/404" => Reply::Status(404),
            "/to-http" => Reply::Redirect(format!("http://feeds.test:{port}/ok.ics")),
            "/to-metadata-literal" => Reply::Redirect("https://169.254.169.254/latest".into()),
            "/to-internal-name" => Reply::Redirect(format!("https://internal.test:{port}/ok.ics")),
            "/to-ok" => Reply::Redirect(format!("https://other.test:{port}/ok.ics")),
            "/loop" => Reply::Redirect(format!("https://feeds.test:{port}/loop")),
            "/big" => Reply::Ok(vec![b'x'; 3 * 1024 * 1024]),
            "/stream-big" => Reply::Stream(3 * 1024 * 1024),
            "/stall" => Reply::Stall,
            _ => Reply::Status(404),
        }
    })
}

async fn server() -> TestServer {
    let port = Arc::new(std::sync::OnceLock::new());
    let s = start(router(port.clone())).await;
    let _ = port.set(s.port);
    s
}

fn fetcher(s: &TestServer, allow_private: bool) -> GuardedFetcher {
    GuardedFetcher::new(FetcherConfig {
        connect_timeout: Duration::from_millis(200),
        total_timeout: Duration::from_millis(500),
        allow_private_hosts: allow_private,
        extra_root_ca: Some(s.ca()),
        resolver: Some(FixedResolver::with(&[
            ("localhost", "127.0.0.1"),
            ("feeds.test", "127.0.0.1"),
            ("other.test", "127.0.0.1"),
            ("internal.test", "169.254.169.254"),
            ("private.test", "10.0.0.5"),
        ])),
        ..FetcherConfig::default()
    })
    .expect("fetcher")
}

#[tokio::test]
async fn the_happy_path_fetches_the_body_with_feed_headers() {
    let s = server().await;
    let body = fetcher(&s, true)
        .fetch(&s.url("feeds.test", "/ok.ics"))
        .await
        .expect("fetched");
    assert_eq!(body, ICS);
    let seen = s.requests();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].headers.get("accept").map(String::as_str),
        Some(FEED_ACCEPT)
    );
    assert_eq!(
        seen[0].headers.get("user-agent").map(String::as_str),
        Some(FEED_USER_AGENT)
    );
}

#[tokio::test]
async fn defaults_refuse_loopback_and_private_before_connecting() {
    let s = server().await;
    let f = fetcher(&s, false);
    for (host, path) in [
        ("127.0.0.1", "/ok.ics"),
        ("[::ffff:7f00:1]", "/ok.ics"),
        ("localhost", "/ok.ics"),
        ("private.test", "/ok.ics"),
        ("169.254.169.254", "/ok.ics"),
    ] {
        let err = f.fetch(&s.url(host, path)).await.expect_err(host);
        assert_eq!(err, FetchError::AddressNotAllowed, "{host}");
    }
    assert!(s.requests().is_empty(), "nothing reached the server");
}

#[tokio::test]
async fn an_ip_literal_is_refused_even_though_it_skips_the_resolver() {
    let s = server().await;
    // No resolver entry exists for a literal; the literal check alone refuses it.
    let err = fetcher(&s, false)
        .fetch(&s.url("127.0.0.1", "/ok.ics"))
        .await
        .expect_err("refused");
    assert_eq!(err, FetchError::AddressNotAllowed);
}

#[tokio::test]
async fn metadata_stays_blocked_with_allow_private() {
    let s = server().await;
    let f = fetcher(&s, true);
    for host in ["169.254.169.254", "internal.test", "[fd00:ec2::254]"] {
        let err = f.fetch(&s.url(host, "/ok.ics")).await.expect_err(host);
        assert_eq!(err, FetchError::AddressNotAllowed, "{host}");
    }
}

#[tokio::test]
async fn http_urls_are_refused() {
    let s = server().await;
    let url = url::Url::parse("http://feeds.test/ok.ics").expect("url");
    assert_eq!(
        fetcher(&s, true).fetch(&url).await,
        Err(FetchError::InvalidUrl)
    );
}

#[tokio::test]
async fn redirects_are_checked_again() {
    let s = server().await;
    let f = fetcher(&s, true);
    assert_eq!(
        f.fetch(&s.url("feeds.test", "/to-http")).await,
        Err(FetchError::Redirect)
    );
    assert_eq!(
        f.fetch(&s.url("feeds.test", "/to-metadata-literal")).await,
        Err(FetchError::Redirect)
    );
    assert_eq!(
        f.fetch(&s.url("feeds.test", "/to-internal-name")).await,
        Err(FetchError::AddressNotAllowed)
    );
    assert_eq!(
        f.fetch(&s.url("feeds.test", "/loop")).await,
        Err(FetchError::Redirect)
    );
    assert_eq!(
        f.fetch(&s.url("feeds.test", "/to-ok")).await.as_deref(),
        Ok(ICS)
    );
}

#[tokio::test]
async fn http_errors_fail() {
    let s = server().await;
    let f = fetcher(&s, true);
    assert_eq!(
        f.fetch(&s.url("feeds.test", "/503")).await,
        Err(FetchError::HttpStatus(503))
    );
    assert_eq!(
        f.fetch(&s.url("feeds.test", "/404")).await,
        Err(FetchError::HttpStatus(404))
    );
}

#[tokio::test]
async fn bodies_over_the_cap_are_refused() {
    let s = server().await;
    let f = fetcher(&s, true);
    assert_eq!(
        f.fetch(&s.url("feeds.test", "/big")).await,
        Err(FetchError::TooLarge)
    );
    assert_eq!(
        f.fetch(&s.url("feeds.test", "/stream-big")).await,
        Err(FetchError::TooLarge)
    );
}

#[tokio::test]
async fn a_stalled_server_times_out() {
    let s = server().await;
    let started = std::time::Instant::now();
    assert_eq!(
        fetcher(&s, true)
            .fetch(&s.url("feeds.test", "/stall"))
            .await,
        Err(FetchError::Timeout)
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn an_untrusted_certificate_fails_to_connect() {
    let s = server().await;
    let f = GuardedFetcher::new(FetcherConfig {
        connect_timeout: Duration::from_millis(500),
        total_timeout: Duration::from_millis(800),
        allow_private_hosts: true,
        resolver: Some(FixedResolver::with(&[("feeds.test", "127.0.0.1")])),
        ..FetcherConfig::default()
    })
    .expect("fetcher");
    assert_eq!(
        f.fetch(&s.url("feeds.test", "/ok.ics")).await,
        Err(FetchError::Connect)
    );
}

#[tokio::test]
async fn errors_never_contain_the_url() {
    let s = server().await;
    let f = fetcher(&s, true);
    for path in ["/503", "/to-http", "/stall", "/big"] {
        let url = s.url("feeds.test", &format!("{path}?s=secret-token"));
        let err = f.fetch(&url).await.expect_err(path);
        let text = format!("{err} {err:?} {}", err.code());
        assert!(!text.contains("secret"), "{text}");
        assert!(
            !text.contains("feeds.test") && !text.contains("https"),
            "{text}"
        );
    }
}
