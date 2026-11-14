//! Serves a real router with the security layer and checks the headers and
//! the cross-origin request policy over HTTP.

use tokio::net::TcpListener;
use topcoat::{
    Result,
    context::Cx,
    router::{Router, RouterBuilderDiscoverExt, page},
    view::{View, view},
};
use web_ui_common::map_config::MapConfig;
use web_ui_common::security_headers::{
    ADMIN_CONTENT_SECURITY_POLICY, CONTENT_SECURITY_POLICY, HTML_CACHE_CONTROL, SecurityHeaders,
    csp_with_storage_origin,
};

#[page("/__probe")]
async fn probe_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! { <p>"probe"</p> })
}

#[page(POST "/__probe")]
async fn probe_post(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! { <p>"posted"</p> })
}

async fn serve(layer: SecurityHeaders) -> String {
    let _ = (probe_page, probe_post);
    let router = Router::builder().discover().layer(layer).build();
    let listener = TcpListener::bind("127.0.0.1:0").await.ok();
    let addr = listener.as_ref().and_then(|l| l.local_addr().ok());
    if let Some(listener) = listener {
        tokio::spawn(async move {
            let _ = topcoat::serve(listener, router).await;
        });
    }
    addr.map(|a| format!("http://{a}")).unwrap_or_default()
}

#[tokio::test]
async fn html_pages_get_csp_hardening_and_private_caching() -> reqwest::Result<()> {
    let base = serve(SecurityHeaders::new()).await;
    let res = reqwest::get(format!("{base}/__probe")).await?;
    let h = res.headers();
    assert_eq!(h["content-security-policy"], CONTENT_SECURITY_POLICY);
    assert!(!CONTENT_SECURITY_POLICY.contains("unsafe-"));
    assert!(CONTENT_SECURITY_POLICY.contains("frame-ancestors 'none'"));
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(h["x-frame-options"], "DENY");
    assert_eq!(h["referrer-policy"], "strict-origin-when-cross-origin");
    assert_eq!(h["cross-origin-opener-policy"], "same-origin");
    assert_eq!(h["cache-control"], HTML_CACHE_CONTROL);
    assert!(h.get_all("vary").iter().any(|v| v == "Cookie"));
    assert!(h.get("strict-transport-security").is_none());
    Ok(())
}

#[tokio::test]
async fn error_pages_get_the_csp_too() -> reqwest::Result<()> {
    let base = serve(SecurityHeaders::new()).await;
    let res = reqwest::get(format!("{base}/no-such-page")).await?;
    assert_eq!(res.status(), 404);
    assert_eq!(
        res.headers()["content-security-policy"],
        CONTENT_SECURITY_POLICY
    );
    Ok(())
}

#[tokio::test]
async fn hsts_is_opt_in() -> reqwest::Result<()> {
    let base = serve(SecurityHeaders::new().with_hsts(true)).await;
    let res = reqwest::get(format!("{base}/__probe")).await?;
    assert!(res.headers().contains_key("strict-transport-security"));
    Ok(())
}

#[tokio::test]
async fn cross_origin_posts_are_rejected() -> reqwest::Result<()> {
    let base = serve(SecurityHeaders::new()).await;
    let client = reqwest::Client::new();
    let foreign = client
        .post(format!("{base}/__probe"))
        .header("origin", "https://evil.example")
        .header("sec-fetch-site", "cross-site")
        .send()
        .await?;
    assert_eq!(foreign.status(), 403);
    let same = client
        .post(format!("{base}/__probe"))
        .header("origin", base.as_str())
        .header("sec-fetch-site", "same-origin")
        .send()
        .await?;
    assert_eq!(same.status(), 200);
    Ok(())
}

#[tokio::test]
async fn admin_csp_only_adds_cloud_storage_uploads() -> reqwest::Result<()> {
    let base = serve(SecurityHeaders::admin()).await;
    let res = reqwest::get(format!("{base}/__probe")).await?;
    assert_eq!(
        res.headers()["content-security-policy"],
        ADMIN_CONTENT_SECURITY_POLICY
    );
    assert!(
        ADMIN_CONTENT_SECURITY_POLICY
            .contains("connect-src 'self' https://storage.googleapis.com;")
    );
    assert!(!CONTENT_SECURITY_POLICY.contains("connect-src 'self' https"));
    assert_eq!(
        ADMIN_CONTENT_SECURITY_POLICY
            .replace(" https://storage.googleapis.com;", ";")
            .replace(
                "img-src 'self' data:",
                "img-src 'self' https://storage.googleapis.com data:"
            ),
        CONTENT_SECURITY_POLICY
    );
    Ok(())
}

#[tokio::test]
async fn a_storage_emulator_origin_is_allowed_alongside_cloud_storage() -> reqwest::Result<()> {
    let origin = Some("http://localhost:4443".to_string());
    let admin = serve(SecurityHeaders::admin().with_storage_origin(origin.clone())).await;
    let guest = serve(SecurityHeaders::new().with_storage_origin(origin)).await;
    let admin_csp =
        reqwest::get(format!("{admin}/__probe")).await?.headers()["content-security-policy"]
            .to_str()
            .unwrap_or_default()
            .to_string();
    let guest_csp =
        reqwest::get(format!("{guest}/__probe")).await?.headers()["content-security-policy"]
            .to_str()
            .unwrap_or_default()
            .to_string();
    assert!(
        admin_csp
            .contains("connect-src 'self' https://storage.googleapis.com http://localhost:4443;")
    );
    assert!(
        admin_csp
            .contains("img-src 'self' https://storage.googleapis.com http://localhost:4443 data:")
    );
    assert!(
        guest_csp
            .contains("img-src 'self' https://storage.googleapis.com http://localhost:4443 data:")
    );
    assert!(guest_csp.contains("connect-src 'self';"));
    Ok(())
}

#[tokio::test]
async fn without_a_storage_origin_the_csp_is_unchanged() -> reqwest::Result<()> {
    let base = serve(SecurityHeaders::admin().with_storage_origin(None)).await;
    let res = reqwest::get(format!("{base}/__probe")).await?;
    assert_eq!(
        res.headers()["content-security-policy"],
        ADMIN_CONTENT_SECURITY_POLICY
    );
    Ok(())
}

#[test]
fn storage_origins_that_could_inject_directives_are_refused() {
    for origin in [
        "",
        "http://x; script-src *",
        "http://a http://b",
        "'unsafe-inline'",
    ] {
        assert_eq!(
            csp_with_storage_origin(CONTENT_SECURITY_POLICY, origin),
            None,
            "{origin}"
        );
    }
}

// ---- Spec 103 §1.12: the map tiles' bucket path ----

const TILES: &str = "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261113";
const BUCKET_PATH: &str = "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/";

async fn csp_of(layer: SecurityHeaders) -> String {
    let base = serve(layer).await;
    match reqwest::get(format!("{base}/__probe")).await {
        Ok(res) => res.headers()["content-security-policy"]
            .to_str()
            .unwrap_or_default()
            .to_string(),
        Err(e) => panic!("probe: {e}"),
    }
}

/// The sources of one directive, or `None` when it's absent.
fn directive<'a>(csp: &'a str, name: &str) -> Option<Vec<&'a str>> {
    csp.split("; ")
        .map(|d| d.split(' ').collect::<Vec<_>>())
        .find(|parts| parts.first() == Some(&name))
        .map(|parts| parts[1..].to_vec())
}

fn tiles() -> MapConfig {
    match MapConfig::parse(TILES, None) {
        Ok(config) => config,
        Err(e) => panic!("{e}"),
    }
}

#[tokio::test]
async fn map_origin_adds_the_bucket_path_to_connect_src_and_a_worker() {
    let map = tiles();
    for (layer, base) in [
        (SecurityHeaders::new(), CONTENT_SECURITY_POLICY),
        (SecurityHeaders::admin(), ADMIN_CONTENT_SECURITY_POLICY),
    ] {
        let csp = csp_of(layer.with_map_origin(Some(&map))).await;
        let connect = directive(&csp, "connect-src").unwrap_or_default();
        assert!(connect.contains(&BUCKET_PATH), "{csp}");
        assert_eq!(directive(&csp, "worker-src"), Some(vec!["'self'"]), "{csp}");
        // Nothing else moves: img-src, script-src and style-src are as before.
        for name in ["img-src", "script-src", "style-src", "default-src"] {
            assert_eq!(directive(&csp, name), directive(base, name), "{name}");
        }
        assert!(!csp.contains("unsafe-"), "{csp}");
        assert!(!csp.contains("blob:"), "{csp}");
    }
}

#[tokio::test]
async fn map_origin_none_leaves_both_policies_byte_identical() {
    for raw in [
        "https://storage.googleapis.com/",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v2026-11",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261101/",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261101?a",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261101#a",
        "https://storage.googleapis.com/our%2Dplaces/v20261101",
        "https://storage.googleapis.com/a..b/v20261101",
        "https://storage.googleapis.com/our-places\tx/v20261101",
        "https://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261101\r",
        "http://storage.googleapis.com/our-places-gcs-map-tiles-dev/v20261101",
        "http://localhost:4443/our-places-gcs-map-tiles-local/v20261101",
    ] {
        let map = MapConfig::parse(raw, None).ok();
        assert_eq!(map, None, "{raw:?}");
        assert_eq!(
            csp_of(SecurityHeaders::new().with_map_origin(map.as_ref())).await,
            CONTENT_SECURITY_POLICY
        );
        assert_eq!(
            csp_of(SecurityHeaders::admin().with_map_origin(map.as_ref())).await,
            ADMIN_CONTENT_SECURITY_POLICY
        );
    }
}

#[tokio::test]
async fn map_origin_with_a_storage_origin_never_widens_guest_connect_src() {
    let map = tiles();
    let csp = csp_of(
        SecurityHeaders::new()
            .with_storage_origin(Some("http://localhost:4443".to_string()))
            .with_map_origin(Some(&map)),
    )
    .await;
    let connect = directive(&csp, "connect-src").unwrap_or_default();
    assert_eq!(connect, vec!["'self'", BUCKET_PATH], "{csp}");
    assert!(
        !connect.contains(&"https://storage.googleapis.com"),
        "{csp}"
    );
    // The storage origin still applies where it did.
    assert!(
        csp.contains("img-src 'self' https://storage.googleapis.com http://localhost:4443 data:"),
        "{csp}"
    );
    assert!(!csp.contains("unsafe-"), "{csp}");
}

#[tokio::test]
async fn an_emulator_map_origin_is_its_own_bucket_path() {
    let map = match MapConfig::parse(
        "http://localhost:4443/our-places-gcs-map-tiles-local/v20261113",
        Some("http://localhost:4443"),
    ) {
        Ok(map) => map,
        Err(e) => panic!("{e}"),
    };
    let csp = csp_of(
        SecurityHeaders::new()
            .with_storage_origin(Some("http://localhost:4443".to_string()))
            .with_map_origin(Some(&map)),
    )
    .await;
    assert_eq!(
        directive(&csp, "connect-src"),
        Some(vec![
            "'self'",
            "http://localhost:4443/our-places-gcs-map-tiles-local/"
        ]),
        "{csp}"
    );
}
