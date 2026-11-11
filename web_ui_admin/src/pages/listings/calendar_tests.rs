//! Render tests for the calendar page sections (spec §1.7).

use super::calendar::{
    blocks_section, conflicts_section, export_section, export_url, feeds_section, outcome_text,
    refusal_message, relative, status_badge,
};
use chrono::{NaiveDate, Utc};
use common::calendar::conflict::StaleReason;
use common::calendar::dto::{
    BlockedDateResponse, CalendarConflictResponse, CalendarConflictsResponse, CalendarFeedResponse,
    FeedSyncResult, Page, SyncOutcome,
};
use common::calendar::{BlockSource, ConflictKind, FeedStatus};
use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use uuid::Uuid;
use web_client::client::{ApiRejection, UserCallError};

async fn html(v: impl topcoat::view::View, cx: &Cx) -> String {
    match v.first().await {
        Ok(handle) => handle.render(cx),
        Err(e) => panic!("render failed: {e}"),
    }
}

fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2030, 1, day).expect("date")
}

fn feed(status: FeedStatus, active: bool) -> CalendarFeedResponse {
    CalendarFeedResponse {
        id: Uuid::now_v7(),
        listing_id: Uuid::nil(),
        name: "Airbnb <main>".into(),
        url: "https://www.airbnb.com/calendar/ical/1.ics?s=x".into(),
        is_active: active,
        last_status: status,
        last_error: (status == FeedStatus::Failed)
            .then(|| "calendar server returned HTTP 503".into()),
        last_warning: None,
        last_attempt_at: Some(Utc::now()),
        last_success_at: None,
        event_count: 3,
        consecutive_failures: 0,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn the_export_section_has_the_link_and_a_copy_button() {
    let cx = &Cx::default();
    let listing = Uuid::now_v7();
    let url = export_url("https://guest.example", listing);
    assert_eq!(
        url,
        format!("https://guest.example/listings/{listing}/calendar.ics")
    );
    let shown = url.clone();
    let out = html(view! { export_section(url: shown) }, cx).await;
    assert!(
        out.contains(&url) && out.contains(r##"data-copy-target="#calendar-export-url""##),
        "{out}"
    );
    assert!(!out.contains("onclick") && !out.contains("<script"));
}

#[tokio::test]
async fn every_feed_status_has_its_badge() {
    let cx = &Cx::default();
    for (status, class) in [
        (FeedStatus::Never, "badge-ghost"),
        (FeedStatus::Ok, "badge-success"),
        (FeedStatus::Warning, "badge-warning"),
        (FeedStatus::Failed, "badge-error"),
    ] {
        assert!(status_badge(status).1.contains(class));
        let out = html(
            view! { feeds_section(listing_id: Uuid::nil(), feeds: vec![feed(status, true)], results: Vec::new(), error: None) },
            cx,
        )
        .await;
        assert!(out.contains(class), "{status:?}: {out}");
        assert!(out.contains("Airbnb &lt;main&gt;"), "names are escaped");
    }
    let out = html(
        view! { feeds_section(listing_id: Uuid::nil(), feeds: vec![feed(FeedStatus::Failed, false)], results: Vec::new(), error: None) },
        cx,
    )
    .await;
    assert!(
        out.contains("HTTP 503") && out.contains("Switch on"),
        "{out}"
    );
}

#[tokio::test]
async fn every_sync_outcome_is_shown() {
    let cx = &Cx::default();
    let f = feed(FeedStatus::Ok, true);
    let results: Vec<FeedSyncResult> = [
        SyncOutcome::Ok,
        SyncOutcome::Warning,
        SyncOutcome::Failed,
        SyncOutcome::Skipped,
        SyncOutcome::Deferred,
    ]
    .into_iter()
    .map(|outcome| FeedSyncResult {
        feed_id: f.id,
        outcome,
        events: None,
        error_code: (outcome == SyncOutcome::Failed).then(|| "fetch_timeout".into()),
        conflicts_opened: 0,
    })
    .collect();
    let out = html(
        view! { feeds_section(listing_id: Uuid::nil(), feeds: vec![f], results: results, error: None) },
        cx,
    )
    .await;
    for outcome in [
        SyncOutcome::Ok,
        SyncOutcome::Warning,
        SyncOutcome::Failed,
        SyncOutcome::Skipped,
        SyncOutcome::Deferred,
    ] {
        assert!(out.contains(outcome_text(outcome)), "{outcome:?}");
    }
    assert!(out.contains("fetch_timeout"));
    assert!(out.contains(r#"hx-disabled-elt="find button""#) && out.contains("hx-indicator"));
}

#[tokio::test]
async fn api_errors_show_inline_and_429_reads_naturally() {
    let cx = &Cx::default();
    let rejection = |status: u16, code: &str, message: &str| {
        UserCallError::Api(ApiRejection {
            status,
            code: code.into(),
            message: message.into(),
            body: serde_json::Value::Null,
        })
    };
    assert_eq!(
        refusal_message(&rejection(429, "CALENDAR_SYNC_RATE_LIMITED", "x")),
        "Synced less than a minute ago. Try again shortly."
    );
    let msg = refusal_message(&rejection(
        409,
        "DATE_UNAVAILABLE",
        "The requested dates are no longer available.",
    ));
    let out = html(
        view! { blocks_section(listing_id: Uuid::nil(), blocks: Vec::new(), error: Some(msg)) },
        cx,
    )
    .await;
    assert!(
        out.contains(r#"role="alert""#) && out.contains("no longer available"),
        "{out}"
    );
}

fn block(
    source: BlockSource,
    feed_name: Option<&str>,
    active: Option<bool>,
) -> BlockedDateResponse {
    BlockedDateResponse {
        id: Uuid::now_v7(),
        listing_id: Uuid::nil(),
        source,
        feed_id: feed_name.map(|_| Uuid::now_v7()),
        feed_name: feed_name.map(str::to_string),
        feed_active: active,
        date_from: d(5),
        date_to: d(8),
        note: (source == BlockSource::Manual).then(|| "owner stay".into()),
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn blocks_are_grouped_and_disabled_feeds_are_labelled() {
    let cx = &Cx::default();
    let blocks = vec![
        block(BlockSource::Manual, None, None),
        block(BlockSource::IcalFeed, Some("Airbnb"), Some(true)),
        block(BlockSource::IcalFeed, Some("VRBO"), Some(false)),
    ];
    let out = html(
        view! { blocks_section(listing_id: Uuid::nil(), blocks: blocks, error: None) },
        cx,
    )
    .await;
    assert!(
        out.contains("owner stay") && out.contains("Remove"),
        "{out}"
    );
    assert!(out.contains("From Airbnb") && out.contains("From VRBO"));
    assert_eq!(out.matches("from a disabled calendar").count(), 1, "{out}");
    assert_eq!(
        out.matches("Remove</button>").count(),
        1,
        "only manual blocks can be removed"
    );
}

fn conflict(
    stale: Option<StaleReason>,
    reopened: bool,
    feed: bool,
    acked: bool,
) -> CalendarConflictResponse {
    CalendarConflictResponse {
        id: Uuid::now_v7(),
        listing_id: Uuid::nil(),
        feed_id: feed.then(Uuid::now_v7),
        feed_name: "Airbnb".into(),
        block_id: None,
        booking_id: Uuid::now_v7(),
        confirmation_code: "ABC123".into(),
        booking_date_from: d(10),
        booking_date_to: d(15),
        kind: ConflictKind::Overlap,
        block_date_from: d(12),
        block_date_to: d(13),
        detected_at: Utc::now(),
        reopened_at: reopened.then(Utc::now),
        reopened,
        acknowledged_at: acked.then(Utc::now),
        acknowledged_by: None,
        stale_reason: stale,
    }
}

#[tokio::test]
async fn conflicts_show_order_stale_labels_and_the_reopened_badge() {
    let cx = &Cx::default();
    let live = conflict(None, true, true, false);
    let removed = conflict(Some(StaleReason::EventRemoved), false, false, false);
    let released = conflict(Some(StaleReason::BookingReleased), false, true, false);
    let conflicts = CalendarConflictsResponse {
        open: vec![live.clone(), removed.clone(), released.clone()],
        history: Page {
            items: vec![conflict(None, false, true, true)],
            page: 1,
            per_page: 20,
            total: 21,
        },
    };
    let out = html(
        view! { conflicts_section(listing_id: Uuid::nil(), conflicts: conflicts, error: None) },
        cx,
    )
    .await;
    let pos = |id: Uuid| out.find(&format!("conflict-{id}")).expect("rendered");
    assert!(pos(live.id) < pos(removed.id) && pos(removed.id) < pos(released.id));
    assert!(out.contains("Reopened"));
    assert!(
        out.contains("calendar removed"),
        "a removed feed's event reads 'calendar removed'"
    );
    assert!(out.contains("booking no longer active"));
    assert!(out.contains("opacity-60"), "stale ones are greyed out");
    assert_eq!(out.matches("Acknowledge</button>").count(), 3);
    assert!(
        out.contains("Acknowledged (21)") && out.contains("page=2"),
        "history paginates"
    );
}

#[test]
fn relative_times_read_naturally() {
    let now = Utc::now();
    assert_eq!(relative(None, now), "never");
    assert_eq!(relative(Some(now), now), "just now");
    assert_eq!(
        relative(Some(now - chrono::Duration::minutes(5)), now),
        "5 min ago"
    );
    assert_eq!(
        relative(Some(now - chrono::Duration::hours(3)), now),
        "3 h ago"
    );
}
