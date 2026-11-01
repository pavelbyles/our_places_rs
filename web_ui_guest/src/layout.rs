use topcoat::{
    Result,
    context::Cx,
    router::layout,
    view::{Child, View},
};
use web_ui_common::shell_guest::guest_shell;

use crate::support::{display_currency, nav_user};

/// Wraps every page in the guest shell (full document, or just the page
/// for HTMX partial requests).
#[layout("/")]
pub async fn guest_layout(
    cx: &Cx,
    /// The page being wrapped.
    slot: Child<'_>,
) -> Result<impl View> {
    let user = nav_user(cx).await;
    guest_shell(
        cx,
        topcoat::tailwind::stylesheet!(),
        "Luxury Jamaica Stays",
        user,
        display_currency(cx),
        slot,
    )
    .await
}
