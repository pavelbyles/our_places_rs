//! Photo names on the listing edit page: one form per photo, "Photo N"
//! placeholders, and names in the suite photo picker.

use common::models::ListingImageResponse;
use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use uuid::Uuid;
use web_ui_admin::pages::listings::images::{
    NameForm, photo_grid, photo_name_form, suite_photo_choices,
};

async fn html(v: impl topcoat::view::View, cx: &Cx) -> String {
    match v.first().await {
        Ok(handle) => handle.render(cx),
        Err(e) => panic!("render failed: {e}"),
    }
}

fn photo(name: Option<&str>) -> ListingImageResponse {
    ListingImageResponse {
        id: Uuid::now_v7(),
        url: "https://storage.googleapis.com/b/listings/x".into(),
        name: name.map(str::to_string),
    }
}

#[tokio::test]
async fn each_photo_gets_a_name_box_with_a_photo_n_placeholder() {
    let cx = &Cx::default();
    let photos = vec![photo(Some("Ocean terrace")), photo(None)];
    let out = html(
        view! {
            photo_grid(
                listing_id: Uuid::now_v7(),
                name: "Sea Grape".to_string(),
                photos: photos,
                error: None
            )
        },
        cx,
    )
    .await;
    assert_eq!(out.matches("data-photo-name-form").count(), 2, "{out}");
    assert!(out.contains(r#"value="Ocean terrace""#), "{out}");
    assert!(out.contains(r#"placeholder="Photo 2""#), "{out}");
    assert!(out.contains(r#"maxlength="80""#), "{out}");
    assert!(
        out.contains(r#"alt="Sea Grape, photo 2""#),
        "unnamed alt falls back: {out}"
    );
    assert!(out.contains(r#"hx-trigger="change, submit""#), "{out}");
}

#[tokio::test]
async fn a_refused_name_keeps_the_text_and_says_why() {
    let cx = &Cx::default();
    let out = html(
        view! {
            photo_name_form(
                listing_id: Uuid::now_v7(),
                image_id: Uuid::now_v7(),
                index: 0,
                value: "x".repeat(81),
                error: Some("A photo name can be at most 80 characters.".to_string())
            )
        },
        cx,
    )
    .await;
    assert!(
        out.contains(r#"role="alert""#) && out.contains("at most 80 characters"),
        "{out}"
    );
    assert!(out.contains(&"x".repeat(81)), "{out}");
    assert!(out.contains(r#"aria-invalid="true""#), "{out}");
}

#[test]
fn the_suite_picker_uses_names_with_a_photo_n_fallback() {
    let photos = vec![photo(None), photo(Some("Master suite")), photo(Some("  "))];
    let labels: Vec<_> = suite_photo_choices(&photos)
        .into_iter()
        .map(|(_, l)| l)
        .collect();
    assert_eq!(labels, ["Photo 1", "Master suite", "Photo 3"]);
}

#[test]
fn the_name_form_posts_parse() {
    let id = Uuid::now_v7();
    let body = format!("image_id={id}&index=2&name=Pool+deck");
    let form: Result<NameForm, _> = serde_urlencoded::from_str(&body);
    let form = form.map_err(|e| e.to_string());
    assert_eq!(
        form.as_ref()
            .map(|f| (f.image_id, f.index, f.name.as_str())),
        Ok((id, 2, "Pool deck"))
    );
}
