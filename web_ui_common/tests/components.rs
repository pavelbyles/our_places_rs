//! Render tests for the custom presentational components.

use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use web_ui_common::components::{
    framed_image::{FrameAspect, framed_image},
    hero::hero,
    stat_tile::stat_tile,
    steps::steps,
    toast::{ToastVariant, toast, toast_region},
};

/// Resolves and renders a view to HTML.
async fn html(v: impl topcoat::view::View, cx: &Cx) -> String {
    match v.first().await {
        Ok(handle) => handle.render(cx),
        Err(e) => panic!("render failed: {e}"),
    }
}

#[tokio::test]
async fn toast_is_a_status_with_escaped_text() {
    let __cx = &Cx::default();
    let html = html(
        view! {  toast(message: "<b>Saved</b>".to_string(), variant: ToastVariant::Success)  },
        __cx,
    )
    .await;
    assert!(html.contains(r#"role="status""#), "{html}");
    assert!(html.contains(r#"data-variant="success""#), "{html}");
    assert!(html.contains("&lt;b&gt;Saved&lt;/b&gt;"), "{html}");
    assert!(!html.contains("<b>"), "{html}");
}

#[tokio::test]
async fn toast_region_is_a_polite_live_region() {
    let __cx = &Cx::default();
    let html = html(view! {  toast_region()  }, __cx).await;
    assert!(
        html.contains(r#"id="toast-region""#) && html.contains(r#"aria-live="polite""#),
        "{html}"
    );
}

#[tokio::test]
async fn steps_mark_only_the_current_step() {
    let __cx = &Cx::default();
    let html = html(
        view! {
           steps(labels: vec!["Dates".into(), "Details".into(), "Confirm".into()], current: 1)
        },
        __cx,
    )
    .await;
    assert_eq!(html.matches(r#"aria-current="step""#).count(), 1, "{html}");
    assert!(html.contains(r#"data-state="complete""#), "{html}");
    assert!(html.contains(r#"data-state="upcoming""#), "{html}");
    let current = html.find(r#"aria-current="step""#).unwrap_or(0);
    let details = html.find("Details").unwrap_or(0);
    assert!(
        current < details,
        "aria-current should sit on the Details step: {html}"
    );
}

#[tokio::test]
async fn stat_tile_renders_optional_hint() {
    let __cx = &Cx::default();
    let with = html(view! {  stat_tile(label: "Active Holds".into(), value: "3".into(), hint: Some("last 24h".into()))  }, __cx).await;
    let __cx = &Cx::default();
    let without = html(
        view! {  stat_tile(label: "Active Holds".into(), value: "3".into())  },
        __cx,
    )
    .await;
    assert!(
        with.contains("last 24h") && with.contains("Active Holds"),
        "{with}"
    );
    assert!(!without.contains("last 24h"), "{without}");
}

#[tokio::test]
async fn framed_image_requires_alt_and_lazy_loads() {
    let __cx = &Cx::default();
    let html = html(view! {
        framed_image(src: "https://storage.googleapis.com/x.jpg".into(), alt: "Pool at dusk".into(), aspect: FrameAspect::Square)
     }, __cx).await;
    assert!(
        html.contains(r#"alt="Pool at dusk""#) && html.contains(r#"loading="lazy""#),
        "{html}"
    );
    assert!(html.contains("aspect-square"), "{html}");
}

#[tokio::test]
async fn hero_renders_headline_as_h1_and_overlays_children() {
    let __cx = &Cx::default();
    let html = html(
        view! {
           hero(
               headline: "Wake up to the sea".into(),
               lead: "Villas across Jamaica".into(),
               image_src: "/hero.jpg".into(),
               image_alt: "Coastline".into(),
               <form id="hero-search"></form>
           )
        },
        __cx,
    )
    .await;
    assert!(
        html.contains("<h1") && html.contains("Wake up to the sea"),
        "{html}"
    );
    assert!(html.contains(r#"id="hero-search""#), "{html}");
}
