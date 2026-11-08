//! Responsive listing photos. image_worker writes every upload as
//! `optimized/<size>/<listing>_<image>.webp` in each size folder, so any one
//! variant's URL gives the others; other URLs render as a plain `<img>`.

use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

/// Size folders image_worker writes, with each variant's pixel width.
const VARIANTS: [(&str, u32); 5] = [
    ("thumbnail", 400),
    ("mobile", 720),
    ("tablet", 1280),
    ("desktop", 1920),
    ("highres", 2560),
];

/// URL of the `size` variant (a folder in [`VARIANTS`]) of an
/// image_worker photo, or `None` if `src` is not one.
pub fn variant_url(src: &str, size: &str) -> Option<String> {
    VARIANTS.iter().find_map(|(folder, _)| {
        let marker = format!("/optimized/{folder}/");
        src.rfind(&marker).map(|at| {
            format!(
                "{}/optimized/{size}/{}",
                &src[..at],
                &src[at + marker.len()..]
            )
        })
    })
}

/// Whether `src` is an image_worker photo, so resized variants exist.
pub fn has_variants(src: &str) -> bool {
    variant_url(src, "mobile").is_some()
}

/// A `srcset` listing every variant of an image_worker photo with its width,
/// so the browser can pick the one closest to the drawn size (`sizes`) times
/// the screen's pixel density; `None` if `src` has no variants.
pub fn variant_srcset(src: &str) -> Option<String> {
    has_variants(src).then(|| {
        VARIANTS
            .iter()
            .filter_map(|(size, width)| variant_url(src, size).map(|url| format!("{url} {width}w")))
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// A lazily loaded image. Photos with resized variants get a `srcset` of all
/// of them, so the browser loads the one nearest the size it is drawn at.
#[component]
pub async fn responsive_image(
    /// Image URL.
    src: String,
    /// Alternative text.
    alt: String,
    /// How wide the image is drawn: an HTML `sizes` value such as
    /// `"(min-width: 768px) 50vw, 100vw"`. Defaults to the full viewport, so
    /// pass it wherever the image is drawn narrower than the screen.
    #[default]
    sizes: Option<String>,
    /// Extra HTML attributes for the `<img>`; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let img_class = class!("size-full object-cover", attrs.remove("class"));
    let sizes = sizes.unwrap_or_else(|| "100vw".to_string());
    Ok(view! {
        if let Some(srcset) = variant_srcset(&src) {
            <img src=(src) srcset=(srcset) sizes=(sizes) alt=(alt) loading="lazy" decoding="async" class=(img_class) (attrs) />
        } else {
            <img src=(src) alt=(alt) loading="lazy" decoding="async" class=(img_class) (attrs) />
        }
    })
}
