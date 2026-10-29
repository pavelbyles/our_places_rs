//! Responsive listing photos. Cloud Storage uploads have `_640`, `_1024`
//! and `_1920` WebP variants (made by image_worker); other URLs render as a
//! plain `<img>`.

use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

/// Whether `src` points at Cloud Storage, where resized variants exist.
pub fn has_variants(src: &str) -> bool {
    src.contains("storage.googleapis.com") || src.starts_with("https://storage.cloud.google.com")
}

/// `srcset` URL for a resized WebP variant of a Cloud Storage image.
pub fn variant_url(src: &str, width: u32) -> String {
    format!("{src}_{width}.webp")
}

/// A lazily loaded image, wrapped in `<picture>` with WebP sources when the
/// image lives in Cloud Storage.
#[component]
pub async fn responsive_image(
    /// Image URL.
    src: String,
    /// Alternative text.
    alt: String,
    /// Extra HTML attributes for the `<img>`; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let img_class = class!("size-full object-cover", attrs.remove("class"));
    let variants = has_variants(&src);
    let large = variant_url(&src, 1920);
    let medium = variant_url(&src, 1024);
    let small = variant_url(&src, 640);
    Ok(view! {
        if variants {
            <picture class="block size-full">
                <source media="(min-width: 1024px)" srcset=(large) type="image/webp" />
                <source media="(min-width: 640px)" srcset=(medium) type="image/webp" />
                <source srcset=(small) type="image/webp" />
                <img src=(src) alt=(alt) loading="lazy" decoding="async" class=(img_class) (attrs) />
            </picture>
        } else {
            <img src=(src) alt=(alt) loading="lazy" decoding="async" class=(img_class) (attrs) />
        }
    })
}
