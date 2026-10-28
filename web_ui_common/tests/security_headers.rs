//! Serves a real router with the security layer and checks the headers and
//! the cross-origin request policy over HTTP.

use tokio::net::TcpListener;
use topcoat::{
    Result,
    context::Cx,
    router::{Router, RouterBuilderDiscoverExt, page},
    view::{View, view},
};
use web_ui_common::security_headers::{
    ADMIN_CONTENT_SECURITY_POLICY, CONTENT_SECURITY_POLICY, HTML_CACHE_CONTROL, SecurityHeaders,
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
