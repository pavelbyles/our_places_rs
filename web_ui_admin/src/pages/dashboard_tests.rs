//! The dashboard calendar card (spec §1.7.2).

use super::dashboard::calendar_card;
use common::calendar::dto::{CalendarOverview, CalendarOverviewListing};
use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use uuid::Uuid;

async fn html(v: impl topcoat::view::View, cx: &Cx) -> String {
    match v.first().await {
        Ok(handle) => handle.render(cx),
        Err(e) => panic!("render failed: {e}"),
    }
}

#[tokio::test]
async fn the_card_shows_counts_and_links() {
    let __cx = &Cx::default();
    let listing = Uuid::now_v7();
    let overview = CalendarOverview {
        open_conflicts: 2,
        failing_feeds: 1,
        listings: vec![CalendarOverviewListing {
            listing_id: listing,
            listing_name: "Villa Azul".into(),
            open_conflicts: 2,
            failing_feeds: 1,
        }],
    };
    let out = html(view! { calendar_card(overview: Some(overview)) }, __cx).await;
    assert!(
        out.contains("2 open conflict(s)") && out.contains("1 failing calendar(s)"),
        "{out}"
    );
    assert!(out.contains(&format!("/admin/listings/{listing}/calendar")));
}

#[tokio::test]
async fn the_card_is_hidden_with_nothing_to_report() {
    let __cx = &Cx::default();
    let out = html(
        view! { calendar_card(overview: Some(CalendarOverview::default())) },
        __cx,
    )
    .await;
    assert!(!out.contains("calendar-card"), "{out}");
}

#[tokio::test]
async fn the_card_is_hidden_when_the_overview_failed() {
    let __cx = &Cx::default();
    let out = html(view! { calendar_card(overview: None) }, __cx).await;
    assert!(!out.contains("calendar-card"), "{out}");
}
