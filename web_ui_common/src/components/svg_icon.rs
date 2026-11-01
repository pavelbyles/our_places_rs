//! A CSP-safe replacement for `topcoat::icon::icon`, which hard-codes an
//! inline `style="vertical-align: -0.125em"` that `style-src 'self'` blocks.
//! Same markup, with the alignment as a Tailwind class.

use topcoat::{
    Result,
    icon::IconData,
    view::{Attributes, Length, View, class, component, view},
};

/// Renders an [`IconData`] as an inline `<svg>`: `1em` square by default and
/// hidden from assistive technology unless `label` is set.
#[component]
pub async fn svg_icon(
    /// The icon to render.
    data: IconData,
    /// Rendered width and height.
    #[into]
    #[default(Length::em(1.0))]
    size: Length,
    /// Accessible label; when empty the icon is `aria-hidden`.
    #[default]
    #[into]
    label: String,
    /// Extra attributes for the `<svg>`; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <svg
            viewBox=(data.view_box())
            width=(size)
            height=(size)
            class=(class!("inline-block align-[-0.125em]", attrs.remove("class")))
            aria-hidden=(label.is_empty().then_some("true"))
            role=((!label.is_empty()).then_some("img"))
            aria-label=((!label.is_empty()).then_some(label))
            (attrs)
        >
            (data.into_body())
        </svg>
    })
}
