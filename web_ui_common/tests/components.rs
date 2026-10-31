//! Render tests for the custom presentational components.

use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use web_ui_common::components::{
    chat::{chat_bubble, chat_thread},
    framed_image::{FrameAspect, framed_image},
    hero::hero,
    star_rating::{star_rating, star_rating_input},
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

#[tokio::test]
async fn star_rating_labels_the_average_or_says_new() {
    let __cx = &Cx::default();
    let rated = html(
        view! { star_rating(rating: Some(4.66), review_count: Some(12)) },
        __cx,
    )
    .await;
    assert!(
        rated.contains(r#"aria-label="Rated 4.7 out of 5""#),
        "{rated}"
    );
    assert!(rated.contains("(12 reviews)"), "{rated}");
    let unrated = html(view! { star_rating(rating: None) }, __cx).await;
    assert!(
        unrated.contains("New") && !unrated.contains("Rated"),
        "{unrated}"
    );
}

#[tokio::test]
async fn star_rating_input_has_five_labelled_required_radios() {
    let __cx = &Cx::default();
    let out = html(
        view! { star_rating_input(name: "value_rating".into(), label: "Value".into(), value: Some(4)) },
        __cx,
    )
    .await;
    assert_eq!(out.matches(r#"type="radio""#).count(), 5, "{out}");
    assert_eq!(out.matches(r#"name="value_rating""#).count(), 5, "{out}");
    assert_eq!(out.matches(r#" checked=""#).count(), 1, "{out}");
    assert!(
        out.contains("<legend") && out.contains("stars for Value"),
        "{out}"
    );
    assert!(
        out.contains(r#"for="value_rating-3""#) && out.contains(r#"id="value_rating-3""#),
        "{out}"
    );
}

#[tokio::test]
async fn chat_thread_is_a_log_and_bubbles_escape_text() {
    let __cx = &Cx::default();
    let out = html(
        view! {
            chat_thread(attrs: topcoat::view::attributes! { id="messages-list" },
                chat_bubble(sender: "Host".into(), sent_at: "Jan 02, 14:00".into(), text: "<script>x</script>".into(), mine: false)
            )
        },
        __cx,
    )
    .await;
    assert!(
        out.contains(r#"role="log""#) && out.contains(r#"id="messages-list""#),
        "{out}"
    );
    assert!(
        out.contains("&lt;script&gt;") && !out.contains("<script>"),
        "{out}"
    );
}
