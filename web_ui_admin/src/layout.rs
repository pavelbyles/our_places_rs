use topcoat::{
    Result,
    context::Cx,
    router::{layout, request::uri},
    view::{Child, View},
};
use web_ui_common::shell_admin::{AdminChrome, admin_shell_with_chrome};

use crate::support::nav_user;

fn is_listing_preview_path(path: &str) -> bool {
    let trimmed = path.trim_end_matches('/');
    let Some(rest) = trimmed.strip_prefix("/admin/listings/") else {
        return false;
    };
    let Some((id, tail)) = rest.split_once('/') else {
        return false;
    };
    !id.is_empty() && matches!(tail, "preview" | "preview/quote" | "preview/calendar")
}

/// Wraps every page in the admin shell (full document, or just the page for
/// HTMX partial requests). Listing preview routes use standalone guest-styled
/// chrome without the admin sidebar or topbar.
#[layout("/")]
pub async fn admin_layout(
    cx: &Cx,
    /// The page being wrapped.
    slot: Child<'_>,
) -> Result<impl View> {
    let path = uri(cx).path().to_string();
    if is_listing_preview_path(&path) {
        return admin_shell_with_chrome(
            cx,
            topcoat::tailwind::stylesheet!(),
            "Listing Preview",
            None,
            AdminChrome::StandalonePreview {
                include_newsreader: true,
            },
            slot,
        )
        .await;
    }
    let user = nav_user(cx).await;
    admin_shell_with_chrome(
        cx,
        topcoat::tailwind::stylesheet!(),
        "Console",
        user,
        AdminChrome::Standard,
        slot,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::is_listing_preview_path;

    #[test]
    fn detects_listing_preview_and_fragment_paths_only() {
        assert!(is_listing_preview_path("/admin/listings/123/preview"));
        assert!(is_listing_preview_path("/admin/listings/123/preview/"));
        assert!(is_listing_preview_path("/admin/listings/123/preview/quote"));
        assert!(is_listing_preview_path(
            "/admin/listings/123/preview/calendar"
        ));
        assert!(!is_listing_preview_path("/admin/listings/123/edit"));
        assert!(!is_listing_preview_path("/admin/listings/preview"));
        assert!(!is_listing_preview_path("/admin/listings"));
    }
}
