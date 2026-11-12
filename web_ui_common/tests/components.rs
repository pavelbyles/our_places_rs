//! Render tests for the custom presentational components.

use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use web_ui_common::components::{
    amenity_group::amenity_groups,
    chat::{chat_bubble, chat_thread},
    framed_image::{FrameAspect, framed_image},
    hero::hero,
    highlights_ribbon::{highlights_ribbon, ribbon_columns},
    listing_hero::{hero_rating_text, listing_hero},
    section_nav::section_nav,
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
    let cx = &Cx::default();
    let html = html(
        view! {
            toast(message: "<b>Saved</b>".to_string(), variant: ToastVariant::Success)
        },
        cx,
    )
    .await;
    assert!(html.contains(r#"role="status""#), "{html}");
    assert!(html.contains(r#"data-variant="success""#), "{html}");
    assert!(html.contains("&lt;b&gt;Saved&lt;/b&gt;"), "{html}");
    assert!(!html.contains("<b>"), "{html}");
}

#[tokio::test]
async fn toast_region_is_a_polite_live_region() {
    let cx = &Cx::default();
    let html = html(view! { toast_region() }, cx).await;
    assert!(
        html.contains(r#"id="toast-region""#) && html.contains(r#"aria-live="polite""#),
        "{html}"
    );
}

#[tokio::test]
async fn steps_mark_only_the_current_step() {
    let cx = &Cx::default();
    let html = html(
        view! {
            steps(
                labels: vec!["Dates".into(), "Details".into(), "Confirm".into()],
                current: 1
            )
        },
        cx,
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
    let cx = &Cx::default();
    let with = html(
        view! {
            stat_tile(
                label: "Active Holds".into(),
                value: "3".into(),
                hint: Some("last 24h".into())
            )
        },
        cx,
    )
    .await;
    let cx = &Cx::default();
    let without = html(
        view! { stat_tile(label: "Active Holds".into(), value: "3".into()) },
        cx,
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
    let cx = &Cx::default();
    let html = html(
        view! {
            framed_image(
                src: "https://storage.googleapis.com/x.jpg".into(),
                alt: "Pool at dusk".into(),
                aspect: FrameAspect::Square
            )
        },
        cx,
    )
    .await;
    assert!(
        html.contains(r#"alt="Pool at dusk""#) && html.contains(r#"loading="lazy""#),
        "{html}"
    );
    assert!(html.contains("aspect-square"), "{html}");
}

#[tokio::test]
async fn hero_renders_headline_as_h1_and_overlays_children() {
    let cx = &Cx::default();
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
        cx,
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
    let cx = &Cx::default();
    let rated = html(
        view! { star_rating(rating: Some(4.66), review_count: Some(12)) },
        cx,
    )
    .await;
    assert!(
        rated.contains(r#"aria-label="Rated 4.7 out of 5""#),
        "{rated}"
    );
    assert!(rated.contains("(12 reviews)"), "{rated}");
    let unrated = html(view! { star_rating(rating: None) }, cx).await;
    assert!(
        unrated.contains("New") && !unrated.contains("Rated"),
        "{unrated}"
    );
}

#[tokio::test]
async fn star_rating_input_has_five_labelled_required_radios() {
    let cx = &Cx::default();
    let out = html(
        view! {
            star_rating_input(
                name: "value_rating".into(),
                label: "Value".into(),
                value: Some(4)
            )
        },
        cx,
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
    let cx = &Cx::default();
    let out = html(
        view! {
            chat_thread(
                attrs: topcoat::view::attributes! { id="messages-list" },
                chat_bubble(
                    sender: "Host".into(),
                    sent_at: "Jan 02, 14:00".into(),
                    text: "<script>x</script>".into(),
                    mine: false
                )
            )
        },
        cx,
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

fn ribbon(items: &[(&str, &str)]) -> common::listing_details::Ribbon {
    use common::listing_details::{Ribbon, RibbonIcon, RibbonItem};
    let items = items
        .iter()
        .map(|(value, label)| RibbonItem {
            icon: RibbonIcon::Beach,
            value: value.to_string(),
            label: label.to_string(),
        })
        .collect::<Vec<_>>();
    match Ribbon::try_from(items) {
        Ok(r) => r,
        Err(e) => panic!("test ribbon is invalid: {e}"),
    }
}

#[tokio::test]
async fn highlights_ribbon_renders_one_item_per_entry_full_bleed() {
    let cx = &Cx::default();
    let r = ribbon(&[
        ("6", "Bedrooms"),
        ("5.5", "Bathrooms"),
        ("Direct", "Beach access"),
        ("1", "Pool"),
    ]);
    let out = html(view! { highlights_ribbon(ribbon: r) }, cx).await;
    assert!(out.contains(r#"data-testid="highlights-ribbon""#), "{out}");
    assert!(out.contains(r#"aria-label="Villa at a glance""#), "{out}");
    assert!(out.contains("full-bleed"), "{out}");
    assert_eq!(out.matches("<li").count(), 4, "{out}");
    assert!(
        out.contains("Direct") && out.contains("Beach access"),
        "{out}"
    );
    assert!(
        out.contains(r#"aria-hidden="true""#),
        "icons are decorative: {out}"
    );
    assert!(out.contains("min-[860px]:grid-cols-4"), "{out}");
}

#[tokio::test]
async fn highlights_ribbon_escapes_host_text() {
    let cx = &Cx::default();
    let r = ribbon(&[("<b>6</b>", "Bedrooms"), ("2", "<i>x</i>"), ("3", "Guests")]);
    let out = html(view! { highlights_ribbon(ribbon: r) }, cx).await;
    assert!(!out.contains("<b>") && !out.contains("<i>"), "{out}");
    assert!(out.contains("&lt;b&gt;6&lt;/b&gt;"), "{out}");
}

#[test]
fn ribbon_columns_never_exceed_three_below_860px() {
    for n in 3..=6 {
        let cols = ribbon_columns(n);
        assert!(cols.starts_with("grid-cols-2"), "{n}: {cols}");
        assert!(!cols.contains("min-[600px]:grid-cols-4"), "{n}: {cols}");
        assert!(cols.contains(&format!("grid-cols-{n}")), "{n}: {cols}");
    }
}

fn amenity_group(title: &str, items: &[&str]) -> common::listing_details::AmenityGroup {
    common::listing_details::AmenityGroup {
        title: title.into(),
        icon: common::listing_details::AmenityIcon::Waves,
        items: items.iter().map(|i| i.to_string()).collect(),
    }
}

#[tokio::test]
async fn amenity_groups_render_titled_cards_in_order() {
    let cx = &Cx::default();
    let groups = vec![
        amenity_group("Villa", &["Air conditioning", "Starlink Wi-Fi"]),
        amenity_group("Health & Wellness", &["Infinity pool"]),
    ];
    let out = html(view! { amenity_groups(groups: groups) }, cx).await;
    assert!(out.contains(r#"data-testid="amenity-groups""#), "{out}");
    assert_eq!(out.matches("data-amenity-group").count(), 2, "{out}");
    let villa = out.find("Villa").unwrap_or(usize::MAX);
    let wellness = out.find("Health &amp; Wellness").unwrap_or(0);
    assert!(villa < wellness, "groups keep host order: {out}");
    assert_eq!(out.matches("<li").count(), 3, "{out}");
}

#[tokio::test]
async fn no_amenity_groups_render_nothing() {
    let cx = &Cx::default();
    let out = html(view! { amenity_groups(groups: vec![]) }, cx).await;
    assert!(!out.contains("amenity-groups"), "{out}");
}

#[tokio::test]
async fn listing_hero_uses_the_first_photo_eagerly_and_links_the_gallery() {
    let cx = &Cx::default();
    let out = html(
        view! {
            listing_hero(
                name: "Sea Grape".to_string(),
                location: "Runaway Bay, Jamaica".to_string(),
                rating: Some(4.9),
                review_count: Some(18),
                photo: Some("https://storage.googleapis.com/b/listings/x".to_string()),
                photo_count: 24
            )
        },
        cx,
    )
    .await;
    assert!(out.contains(r#"<h1 id="listing-title""#), "{out}");
    assert!(
        out.contains(r#"loading="eager""#) && out.contains(r#"fetchpriority="high""#),
        "{out}"
    );
    assert!(out.contains("View all 24 photos"), "{out}");
    assert!(
        out.contains(r#"data-dialog-open="gallery-dialog""#),
        "{out}"
    );
    assert!(out.contains("full-bleed"), "{out}");
}

#[tokio::test]
async fn listing_hero_without_photos_is_a_title_band() {
    let cx = &Cx::default();
    let out = html(
        view! {
            listing_hero(
                name: "Sea Grape".to_string(),
                location: "Jamaica".to_string(),
                rating: None,
                review_count: None,
                photo: None,
                photo_count: 0
            )
        },
        cx,
    )
    .await;
    assert!(
        !out.contains("<img") && !out.contains("gallery-dialog"),
        "{out}"
    );
    assert!(out.contains("bg-card"), "{out}");
}

#[tokio::test]
async fn section_nav_links_each_section_and_the_booking_card() {
    let cx = &Cx::default();
    let out = html(
        view! {
            section_nav(
                links: vec![("overview", "Overview"), ("reviews", "Reviews")],
                price: "US$650".to_string()
            )
        },
        cx,
    )
    .await;
    assert!(out.contains(r#"aria-label="On this page""#), "{out}");
    assert!(
        out.contains(r##"href="#overview""##) && out.contains(r##"href="#reviews""##),
        "{out}"
    );
    assert!(
        out.contains(r##"href="#book""##) && out.contains("US$650"),
        "{out}"
    );
    assert!(out.contains("sticky top-0"), "{out}");
}

#[test]
fn hero_rating_reads_naturally() {
    assert_eq!(hero_rating_text(Some(4.87), Some(18)), "4.9 · 18 reviews");
    assert_eq!(hero_rating_text(Some(5.0), Some(1)), "5.0 · 1 review");
    assert_eq!(hero_rating_text(Some(4.0), None), "4.0");
    assert_eq!(hero_rating_text(None, Some(0)), "New listing");
}
