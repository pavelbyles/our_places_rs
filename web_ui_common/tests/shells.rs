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
    }
}

#[tokio::test]
async fn signed_out_guest_gets_no_account_menu() {
    let __cx = &Cx::default();
    let out = html(
        view! { guest_chrome(shell: guest(None), <p>"page"</p>) },
        __cx,
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
    let __cx = &Cx::default();
    let user = NavUser {
        name: "Casey <E2E>".into(),
        email: "casey@example.com".into(),
    };
    let out = html(view! { guest_chrome(shell: guest(Some(user))) }, __cx).await;
    assert!(
        out.contains(r#"id="nav-user-dropdown""#) && out.contains("Casey &lt;E2E&gt;"),
        "{out}"
    );
    assert!(out.contains(r#"method="post" action="/logout""#), "{out}");
    assert!(!out.contains("nav-guest-actions"), "{out}");
}

#[tokio::test]
async fn theme_toggle_names_the_next_theme() {
    let __cx = &Cx::default();
    let out = html(view! { guest_chrome(shell: guest(None)) }, __cx).await;
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
        }),
        theme: ThemeChoice::Light,
    }
}

#[tokio::test]
async fn hosts_do_not_see_user_management() {
    let __cx = &Cx::default();
    let host = html(
        view! { admin_chrome(shell: admin(Some(AdminRole::Host))) },
        __cx,
    )
    .await;
    assert!(!host.contains("/admin/users"), "{host}");
    assert!(
        host.contains(r#"href="/admin/bookings""#) && host.contains("Host"),
        "{host}"
    );
    let full = html(
        view! { admin_chrome(shell: admin(Some(AdminRole::Admin))) },
        __cx,
    )
    .await;
    assert!(
        full.contains(r#"id="admin-sidebar-users-link""#) && full.contains("Administrator"),
        "{full}"
    );
}

#[tokio::test]
async fn signed_out_admin_pages_have_no_console_navigation() {
    let __cx = &Cx::default();
    let out = html(
        view! { admin_chrome(shell: admin(None), <form id="login"></form>) },
        __cx,
    )
    .await;
    assert!(
        !out.contains("/admin/bookings") && !out.contains("/logout"),
        "{out}"
    );
    assert!(out.contains(r#"id="login""#), "{out}");
}
