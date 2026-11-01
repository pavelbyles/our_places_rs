use topcoat::{
    Result,
    context::Cx,
    router::layout,
    view::{Child, View},
};
use web_ui_common::shell_admin::admin_shell;

use crate::support::nav_user;

/// Wraps every page in the admin shell (full document, or just the page for
/// HTMX partial requests).
#[layout("/")]
pub async fn admin_layout(
    cx: &Cx,
    /// The page being wrapped.
    slot: Child<'_>,
) -> Result<impl View> {
    let user = nav_user(cx).await;
    admin_shell(cx, topcoat::tailwind::stylesheet!(), "Console", user, slot).await
}
