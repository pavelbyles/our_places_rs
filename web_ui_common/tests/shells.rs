//! Render tests for the guest and admin chrome (spec AC 7: a signed-out
//! guest never receives signed-in navigation; hosts never see user admin).

use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use web_client::session::AdminRole;
use web_ui_common::shell_admin::{AdminNavUser, AdminShell, admin_chrome};
use web_ui_common::shell_guest::{GuestShell, NavUser, guest_chrome};
use web_ui_common::theme::ThemeChoice;

async fn html(v: impl topcoat::view::View, cx: &Cx) -> String {
    match v.first().await {
        Ok(handle) => handle.render(cx),
        Err(e) => panic!("render failed: {e}"),
    }
}

fn guest(user: Option<NavUser>) -> GuestShell {
    GuestShell {
        title: "Home".into(),
        user,
        theme: ThemeChoice::Dim,
        currency: None,
        return_to: "/".into(),
    }
}

#[tokio::test]
async fn signed_out_guest_gets_no_account_menu() {
    let cx = &Cx::default();
    let out = html(
        view! { guest_chrome(shell: guest(None), <p>"page"</p>) },
        cx,
    )
    .await;
    assert!(!out.contains("nav-user-dropdown"), "{out}");
    assert!(
        !out.contains("/logout") && !out.contains("My bookings"),
        "{out}"
    );
    assert!(
        out.contains(r#"id="nav-guest-actions""#) && out.contains(r#"href="/login""#),
        "{out}"
    );
    assert!(
        out.contains(r#"<main id="main""#) && out.contains(r#"id="toast-region""#),
        "{out}"
    );
}

#[tokio::test]
async fn signed_in_guest_gets_account_menu_and_post_logout() {
    let cx = &Cx::default();
    let user = NavUser {
        name: "Casey <E2E>".into(),
        email: "casey@example.com".into(),
    };
    let out = html(view! { guest_chrome(shell: guest(Some(user))) }, cx).await;
    assert!(
        out.contains(r#"id="nav-user-dropdown""#) && out.contains("Casey &lt;E2E&gt;"),
        "{out}"
    );
    assert!(out.contains(r#"method="post" action="/logout""#), "{out}");
    assert!(!out.contains("nav-guest-actions"), "{out}");
}

#[tokio::test]
async fn theme_toggle_names_the_next_theme() {
    let cx = &Cx::default();
    let out = html(view! { guest_chrome(shell: guest(None)) }, cx).await;
    assert!(
        out.contains(r#"aria-label="Switch to dark theme""#),
        "dim -> dark: {out}"
    );
    assert!(out.contains("data-theme-cycle"), "{out}");
}

fn admin(role: Option<AdminRole>) -> AdminShell {
    AdminShell {
        title: "Dashboard".into(),
        user: role.map(|role| AdminNavUser {
            name: "Sam".into(),
            email: "sam@x.io".into(),
            role,
            unread_messages: 0,
        }),
        theme: ThemeChoice::Light,
    }
}

#[test]
fn unread_badge_label_hides_at_zero_and_caps_at_99() {
    use web_ui_common::shell_admin::unread_badge_label;
    assert_eq!(unread_badge_label(0), None);
    assert_eq!(unread_badge_label(-1), None);
    assert_eq!(unread_badge_label(7).as_deref(), Some("(7)"));
    assert_eq!(unread_badge_label(99).as_deref(), Some("(99)"));
    assert_eq!(unread_badge_label(100).as_deref(), Some("(99+)"));
}

#[tokio::test]
async fn admin_nav_shows_unread_guest_messages_on_inbox() {
    let cx = &Cx::default();
    let quiet_shell = admin(Some(AdminRole::Host));
    let quiet = html(view! { admin_chrome(shell: quiet_shell) }, cx).await;
    assert!(
        !quiet.contains("admin-nav-unread"),
        "no badge at 0: {quiet}"
    );

    let mut shell = admin(Some(AdminRole::Host));
    if let Some(user) = shell.user.as_mut() {
        user.unread_messages = 3;
    }
    let out = html(view! { admin_chrome(shell: shell) }, cx).await;
    assert!(out.contains(r#"id="admin-nav-unread""#), "{out}");
    assert!(out.contains(r#"id="admin-mobile-nav-unread""#), "{out}");
    assert!(out.contains(r#"aria-label="3 unread messages""#), "{out}");
    assert!(out.contains(">(3)</span>"), "reads as Inbox (3): {out}");
}

#[tokio::test]
async fn hosts_do_not_see_user_management() {
    let cx = &Cx::default();
    let host = html(
        view! { admin_chrome(shell: admin(Some(AdminRole::Host))) },
        cx,
    )
    .await;
    assert!(!host.contains("/admin/users"), "{host}");
    assert!(
        host.contains(r#"href="/admin/bookings""#) && host.contains("Host"),
        "{host}"
    );
    let full = html(
        view! { admin_chrome(shell: admin(Some(AdminRole::Admin))) },
        cx,
    )
    .await;
    assert!(
        full.contains(r#"id="admin-sidebar-users-link""#) && full.contains("Administrator"),
        "{full}"
    );
}

#[tokio::test]
async fn signed_out_admin_pages_have_no_console_navigation() {
    let cx = &Cx::default();
    let out = html(
        view! { admin_chrome(shell: admin(None), <form id="login"></form>) },
        cx,
    )
    .await;
    assert!(
        !out.contains("/admin/bookings") && !out.contains("/logout"),
        "{out}"
    );
    assert!(out.contains(r#"id="login""#), "{out}");
}
