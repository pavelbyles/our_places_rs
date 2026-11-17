//! Topcoat 0.12 behaviour that spec 102 relies on (plan T1 spike, kept as a
//! regression guard for Topcoat upgrades). Served over HTTP through a layout
//! that renders `topcoat::metadata::tags()` like the guest shell does.
//!
//! - A page error keeps its status even when the layout renders `tags()`.
//! - A metadata loader error can override the page's status, so loaders
//!   must swallow their errors (`Ok(Metadata::new())`).
//! - Error responses are Topcoat's plain bodies, not rendered through layouts.
//! - `Unescaped` inside `<script type="application/ld+json">` is verbatim.
//! - Site-relative metadata URLs panic (500) without a `BaseUrl`.

use tokio::net::TcpListener;
use topcoat::{
    Result,
    context::{BaseUrl, Cx},
    metadata::Metadata,
    router::{
        Router, RouterBuilderDiscoverExt, Slot,
        error::{internal_server_error, not_found},
        layout, page,
    },
    view::{Unescaped, View, view},
};

/// JSON escape for `<`, built at runtime so no tool rewrites the source.
const LT: &str = concat!("\\", "u003c");
/// JSON escape for `>`.
const GT: &str = concat!("\\", "u003e");

#[layout("/")]
async fn contract_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <!DOCTYPE html>
        <html>
            <head>topcoat::metadata::tags()</head>
            <body>(slot)</body>
        </html>
    })
}

fn api_down() -> topcoat::Error {
    internal_server_error(std::io::Error::other("api down")).into()
}

async fn meta_not_found(_cx: &Cx) -> Result<Metadata> {
    Err(not_found().into())
}

async fn meta_server_error(_cx: &Cx) -> Result<Metadata> {
    Err(api_down())
}

async fn meta_ok(_cx: &Cx) -> Result<Metadata> {
    Ok(Metadata::new())
}

#[page("/c/both-404", with(Metadata::from_fn(meta_not_found)))]
async fn both_404(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Err::<String, _>(not_found().into()).map(|s| view! { (s) })
}

#[page("/c/both-500", with(Metadata::from_fn(meta_server_error)))]
async fn both_500(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Err::<String, _>(api_down()).map(|s| view! { (s) })
}

#[page("/c/ok-meta-404", with(Metadata::from_fn(meta_ok)))]
async fn ok_meta_404(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Err::<String, _>(not_found().into()).map(|s| view! { (s) })
}

#[page("/c/ok-meta-500", with(Metadata::from_fn(meta_ok)))]
async fn ok_meta_500(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Err::<String, _>(api_down()).map(|s| view! { (s) })
}

#[page("/c/meta-500-page-404", with(Metadata::from_fn(meta_server_error)))]
async fn meta_500_page_404(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Err::<String, _>(not_found().into()).map(|s| view! { (s) })
}

/// The JSON-LD payload the `/c/json-ld` page emits.
fn json_payload() -> String {
    format!(r#"{{"name":"Villa & Co {LT}/script{GT}"}}"#)
}

#[page("/c/json-ld")]
async fn json_ld_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <script type="application/ld+json">(Unescaped::new_unchecked(json_payload()))</script>
    })
}

#[page("/c/canonical", with(Metadata::new().canonical("/c/canonical")))]
async fn canonical(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! { <p>"canonical"</p> })
}

async fn serve(base: Option<&str>) -> String {
    let _ = (
        contract_layout,
        both_404,
        both_500,
        ok_meta_404,
        ok_meta_500,
        meta_500_page_404,
        json_ld_page,
        canonical,
    );
    let builder = Router::builder().discover();
    let builder = match base.map(BaseUrl::new) {
        Some(Ok(b)) => builder.with(b),
        _ => builder,
    };
    let router = builder.build();
    let listener = TcpListener::bind("127.0.0.1:0").await.ok();
    let addr = listener.as_ref().and_then(|l| l.local_addr().ok());
    if let Some(listener) = listener {
        tokio::spawn(async move {
            let _ = topcoat::serve(listener, router).await;
        });
    }
    addr.map(|a| format!("http://{a}")).unwrap_or_default()
}

async fn get(url: String) -> reqwest::Result<(u16, String)> {
    let res = reqwest::get(url).await?;
    let status = res.status().as_u16();
    Ok((status, res.text().await?))
}

#[tokio::test]
async fn page_404_survives_a_failing_loader() -> reqwest::Result<()> {
    let base = serve(Some("https://example.com")).await;
    assert_eq!(get(format!("{base}/c/both-404")).await?.0, 404);
    Ok(())
}

#[tokio::test]
async fn page_500_survives_a_failing_loader() -> reqwest::Result<()> {
    let base = serve(Some("https://example.com")).await;
    assert_eq!(get(format!("{base}/c/both-500")).await?.0, 500);
    Ok(())
}

#[tokio::test]
async fn swallowing_loader_keeps_the_page_status() -> reqwest::Result<()> {
    let base = serve(Some("https://example.com")).await;
    assert_eq!(get(format!("{base}/c/ok-meta-404")).await?.0, 404);
    assert_eq!(get(format!("{base}/c/ok-meta-500")).await?.0, 500);
    Ok(())
}

/// Why listing metadata must swallow load errors: a loader error overrides
/// the page's own (more accurate) status.
#[tokio::test]
async fn loader_error_overrides_the_page_status() -> reqwest::Result<()> {
    let base = serve(Some("https://example.com")).await;
    assert_eq!(get(format!("{base}/c/meta-500-page-404")).await?.0, 500);
    Ok(())
}

#[tokio::test]
async fn error_responses_are_not_rendered_through_layouts() -> reqwest::Result<()> {
    let base = serve(Some("https://example.com")).await;
    let (_, body) = get(format!("{base}/c/both-404")).await?;
    assert!(!body.contains("<head>"), "{body}");
    Ok(())
}

#[tokio::test]
async fn unescaped_json_ld_is_emitted_verbatim() -> reqwest::Result<()> {
    let base = serve(Some("https://example.com")).await;
    let (status, body) = get(format!("{base}/c/json-ld")).await?;
    assert_eq!(status, 200);
    let expected = format!(
        r#"<script type="application/ld+json">{}</script>"#,
        json_payload()
    );
    assert!(body.contains(&expected), "{body}");
    assert!(!body.contains("&amp;"), "{body}");
    Ok(())
}

#[tokio::test]
async fn relative_canonical_resolves_against_base_url() -> reqwest::Result<()> {
    let base = serve(Some("https://example.com")).await;
    let (status, body) = get(format!("{base}/c/canonical")).await?;
    assert_eq!(status, 200);
    assert!(
        body.contains(r#"<link rel="canonical" href="https://example.com/c/canonical">"#),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn relative_canonical_without_base_url_is_a_500() -> reqwest::Result<()> {
    let base = serve(None).await;
    assert_eq!(get(format!("{base}/c/canonical")).await?.0, 500);
    Ok(())
}
