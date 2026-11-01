//! A rounded photo frame with a placeholder fill while the image loads.

use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

/// Aspect ratio of a [`framed_image`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameAspect {
    /// 4:5 portrait, for villa cards.
    #[default]
    Portrait,
    /// 1:1, for destination tiles.
    Square,
    /// 16:9 landscape.
    Landscape,
}

impl FrameAspect {
    fn class(self) -> &'static str {
        match self {
            Self::Portrait => "aspect-[4/5]",
            Self::Square => "aspect-square",
            Self::Landscape => "aspect-video",
        }
    }
}

/// An image cropped to `aspect` with 12px corners. `alt` is required; pass
/// an empty string only for decorative images.
#[component]
pub async fn framed_image(
    /// Image URL.
    src: String,
    /// Alternative text.
    alt: String,
    /// Crop aspect ratio.
    #[default]
    aspect: FrameAspect,
    /// Extra HTML attributes for the `<img>`; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <div class=(class!("overflow-hidden rounded-[var(--radius-media,12px)] bg-photo-placeholder", aspect.class()))>
            <img
                src=(src)
                alt=(alt)
                loading="lazy"
                decoding="async"
                class=(class!("size-full object-cover", attrs.remove("class")))
                (attrs)
            />
        </div>
    })
}
