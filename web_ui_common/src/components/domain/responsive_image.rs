//! Responsive listing photos. image_worker writes every upload as
//! `optimized/<size>/<listing>_<image>.webp` in each size folder, so any one
//! variant's URL gives the others; other URLs render as a plain `<img>`.

use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

/// Size folders image_worker writes (400, 720, 1280, 1920 and 2560 px wide).
const SIZE_FOLDERS: [&str; 5] = ["thumbnail", "mobile", "tablet", "desktop", "highres"];

/// The `<source>` sizes used, widest first, with the viewport each serves.
const SOURCES: [(&str, Option<&str>); 3] = [
    ("desktop", Some("(min-width: 1024px)")),
    ("tablet", Some("(min-width: 640px)")),
    ("mobile", None),
];

/// URL of the `size` variant (a folder in [`SIZE_FOLDERS`]) of an
/// image_worker photo, or `None` if `src` is not one.
pub fn variant_url(src: &str, size: &str) -> Option<String> {
    SIZE_FOLDERS.iter().find_map(|folder| {
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

/// A lazily loaded image, wrapped in `<picture>` with WebP sources when the
/// image has resized variants.
#[component]
pub async fn responsive_image(
    /// Image URL.
    src: String,
    /// Alternative text.
    alt: String,
    /// Largest variant to load (a size in [`SOURCES`], e.g. `"mobile"`), for
    /// images shown small on wide screens such as fixed-width cards.
    #[default]
    widest: Option<String>,
    /// Extra HTML attributes for the `<img>`; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let img_class = class!("size-full object-cover", attrs.remove("class"));
    let start = widest
        .and_then(|w| SOURCES.iter().position(|(size, _)| *size == w))
        .unwrap_or(0);
    // Dropping the wider sources leaves the next one matching wide viewports.
    let sources: Vec<(String, Option<&str>)> = SOURCES[start..]
        .iter()
        .filter_map(|(size, media)| variant_url(&src, size).map(|url| (url, *media)))
        .collect();
    Ok(view! {
        if sources.is_empty() {
            <img src=(src) alt=(alt) loading="lazy" decoding="async" class=(img_class) (attrs) />
        } else {
            <picture class="block size-full">
                for (srcset, media) in sources {
                    if let Some(media) = media {
                        <source media=(media) srcset=(srcset) type="image/webp" />
                    } else {
                        <source srcset=(srcset) type="image/webp" />
                    }
                }
                <img src=(src) alt=(alt) loading="lazy" decoding="async" class=(img_class) (attrs) />
            </picture>
        }
    })
}
