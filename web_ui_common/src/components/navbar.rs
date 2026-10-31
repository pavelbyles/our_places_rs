//! Header controls shared by both portals.

use topcoat::{
    Result,
    icon::iconify::iconify_icon,
    view::{Attributes, View, attributes, class, component, view},
};

use crate::components::svg_icon::svg_icon;

use crate::theme::ThemeChoice;

/// The 44px pill that cycles light → dim → dark (`ui.js`). Its label names
/// the theme it switches to.
#[component]
pub async fn theme_toggle(
    /// The theme the page was rendered with.
    theme: ThemeChoice,
    /// Extra HTML attributes for the button; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <button
            type="button"
            data-theme-cycle=""
            data-theme-current=(theme.cookie_value())
            aria-label=(theme.next().toggle_label())
            class=(class!("inline-flex size-11 items-center justify-center rounded-full border border-border text-foreground", attrs.remove("class")))
            (attrs)
        >
            svg_icon(data: iconify_icon!("lucide:moon"), attrs: attributes! { class="size-5" aria-hidden="true" })
        </button>
    })
}
