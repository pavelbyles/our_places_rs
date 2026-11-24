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
    let cx = &Cx::default();
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
    let out = html(view! { calendar_card(overview: Some(overview)) }, cx).await;
    assert!(
        out.contains("2 open conflict(s)") && out.contains("1 failing calendar(s)"),
        "{out}"
    );
    assert!(out.contains(&format!("/admin/listings/{listing}/calendar")));
}

#[tokio::test]
async fn the_card_is_hidden_with_nothing_to_report() {
    let cx = &Cx::default();
    let out = html(
        view! { calendar_card(overview: Some(CalendarOverview::default())) },
        cx,
    )
    .await;
    assert!(!out.contains("calendar-card"), "{out}");
}

#[tokio::test]
async fn the_card_is_hidden_when_the_overview_failed() {
    let cx = &Cx::default();
    let out = html(view! { calendar_card(overview: None) }, cx).await;
    assert!(!out.contains("calendar-card"), "{out}");
}

#[test]
fn staff_default_landing_and_post_login_target_are_scoped_to_turnovers() {
    use crate::support::{default_landing_path, post_login_target};
    use web_client::session::AdminRole;

    assert_eq!(default_landing_path(AdminRole::Staff), "/admin/turnovers");
    assert_eq!(default_landing_path(AdminRole::Host), "/admin");
    assert_eq!(default_landing_path(AdminRole::Admin), "/admin");

    assert_eq!(post_login_target(AdminRole::Staff, ""), "/admin/turnovers");
    assert_eq!(post_login_target(AdminRole::Staff, "/"), "/admin/turnovers");
    assert_eq!(
        post_login_target(AdminRole::Staff, "/admin/bookings"),
        "/admin/turnovers"
    );
    assert_eq!(
        post_login_target(AdminRole::Staff, "/admin/turnovers-evil"),
        "/admin/turnovers"
    );
    assert_eq!(
        post_login_target(AdminRole::Staff, "/admin/turnovers"),
        "/admin/turnovers"
    );
    assert_eq!(
        post_login_target(AdminRole::Staff, "/admin/turnovers?window=today"),
        "/admin/turnovers?window=today"
    );
    assert_eq!(
        post_login_target(AdminRole::Staff, "/admin/turnovers/123#comment-1"),
        "/admin/turnovers/123#comment-1"
    );
    assert_eq!(post_login_target(AdminRole::Host, "/"), "/admin");
    assert_eq!(
        post_login_target(AdminRole::Host, "/admin/bookings"),
        "/admin/bookings"
    );
}

#[tokio::test]
async fn reset_password_card_renders_inline_recovery_prompt_when_expired_or_invalid() {
    use super::password_reset::{ResetCardState, reset_password_card};

    let cx = &Cx::default();
    let out = html(
        view! {
            reset_password_card(state: ResetCardState::ExpiredOrInvalid)
        },
        cx,
    )
    .await;
    assert!(
        out.contains(r#"id="reset-password-expired""#)
            && out.contains(r#"href="/forgot-password""#),
        "expected inline recovery prompt linking to /forgot-password: {out}"
    );
    assert!(
        !out.contains("<form"),
        "expired prompt hides password form: {out}"
    );
}

#[tokio::test]
async fn login_form_links_to_forgot_password() {
    use super::auth::login_form;

    let cx = &Cx::default();
    let out = html(
        view! { login_form(email: String::new(), redirect: String::new(), notice: None, error: None) },
        cx,
    )
    .await;
    assert!(
        out.contains(r#"href="/forgot-password""#) && out.contains("Forgot password?"),
        "expected Forgot password? link on login form: {out}"
    );
}
