//! Spec 102 §1.5 (U10): the guest's site-wide head metadata, served over
//! HTTP through a layout that renders `topcoat::metadata::tags()` first in
//! `<head>`, as the guest shell does. The default Open Graph image is an
//! asset and needs the asset bundle, so it is checked end to end (E6).

use tokio::net::TcpListener;
use topcoat::{
    Result,
    context::{BaseUrl, Cx},
    metadata::Metadata,
    router::{Router, RouterBuilderDiscoverExt, Slot, layout, page},
    view::{View, view},
};
use web_ui_common::site_config::{
    GUEST_PORTAL_URL_ENV, SEO_TWITTER_SITE_ENV, SITE_DESCRIPTION, SiteConfig, site_metadata,
};

#[layout("/")]
async fn probe_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <!DOCTYPE html>
        <html>
            <head>topcoat::metadata::tags()</head>
            <body>(slot)</body>
        </html>
    })
}

#[page("/")]
async fn home(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! { <p>"home"</p> })
}

#[page("/listings")]
async fn listings(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! { <p>"listings"</p> })
}

#[page("/about", with(Metadata::new().title("About · Our Places")))]
async fn about(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! { <p>"about"</p> })
}

fn site(twitter: Option<&str>) -> SiteConfig {
    let lookup = move |key: &str| match key {
        k if k == GUEST_PORTAL_URL_ENV => Some("https://ourplaces.io".to_string()),
        k if k == SEO_TWITTER_SITE_ENV => twitter.map(str::to_string),
        _ => None,
    };
    match SiteConfig::from_lookup(lookup) {
        Ok(site) => site,
        Err(e) => panic!("site config: {e}"),
    }
}

async fn serve(site: SiteConfig) -> String {
    let _ = (probe_layout, home, listings, about);
    let router = Router::builder()
        .discover()
        .with(site.base_url().clone())
        .with(site)
        .with(Metadata::from_fn(site_metadata))
        .build();
    let listener = TcpListener::bind("127.0.0.1:0").await.ok();
    let addr = listener.as_ref().and_then(|l| l.local_addr().ok());
    if let Some(listener) = listener {
        tokio::spawn(async move {
            let _ = topcoat::serve(listener, router).await;
        });
    }
    addr.map(|a| format!("http://{a}")).unwrap_or_default()
}

async fn page(site: SiteConfig, path: &str) -> reqwest::Result<String> {
    let base = serve(site).await;
    reqwest::get(format!("{base}{path}")).await?.text().await
}

#[tokio::test]
async fn charset_comes_first_and_there_is_one_title() -> reqwest::Result<()> {
    let html = page(site(None), "/").await?;
    let charset = html.find(r#"<meta charset="utf-8">"#);
    assert!(charset.is_some_and(|at| at < 1024), "{html}");
    assert_eq!(html.matches("<title>").count(), 1, "{html}");
    assert!(
        html.contains("<title>Luxury Jamaica Stays · Our Places</title>"),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn default_description_and_site_name() -> reqwest::Result<()> {
    let html = page(site(None), "/").await?;
    assert!(
        html.contains(&format!(
            r#"<meta name="description" content="{SITE_DESCRIPTION}">"#
        )),
        "{html}"
    );
    assert!(
        html.contains(r#"<meta property="og:site_name" content="Our Places">"#),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn canonical_is_absolute_and_drops_the_query() -> reqwest::Result<()> {
    let html = page(site(None), "/listings?q=negril&check_in=2027-01-01").await?;
    assert!(
        html.contains(r#"<link rel="canonical" href="https://ourplaces.io/listings">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<meta property="og:url" content="https://ourplaces.io/listings">"#),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn a_double_slash_path_never_gives_a_foreign_canonical() -> reqwest::Result<()> {
    // Not routed, but the canonical is computed from the path; prove it can
    // only ever be on the configured origin.
    let base = serve(site(None)).await;
    let html = reqwest::get(format!("{base}//evil.example/x"))
        .await?
        .text()
        .await?;
    assert!(!html.contains(r#"href="//evil.example"#), "{html}");
    assert!(!html.contains(r#"content="//evil.example"#), "{html}");
    Ok(())
}

#[tokio::test]
async fn pages_override_the_title() -> reqwest::Result<()> {
    let html = page(site(None), "/about").await?;
    assert!(html.contains("<title>About · Our Places</title>"), "{html}");
    assert!(
        html.contains(r#"<link rel="canonical" href="https://ourplaces.io/about">"#),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn x_card_is_large_and_names_the_site_only_when_configured() -> reqwest::Result<()> {
    let html = page(site(None), "/").await?;
    assert!(
        html.contains(r#"<meta name="twitter:card" content="summary_large_image">"#),
        "{html}"
    );
    assert!(!html.contains("twitter:site"), "{html}");
    let html = page(site(Some("@ourplaces")), "/").await?;
    assert!(
        html.contains(r#"<meta name="twitter:site" content="@ourplaces">"#),
        "{html}"
    );
    Ok(())
}

#[test]
fn site_metadata_canonical_path_is_same_origin() {
    // The pure helper behind the canonical: any path stays on the origin.
    let base = match BaseUrl::new("https://ourplaces.io") {
        Ok(b) => b,
        Err(e) => panic!("{e}"),
    };
    assert_eq!(
        base.join("//evil.example/x"),
        "https://ourplaces.io/evil.example/x"
    );
}
