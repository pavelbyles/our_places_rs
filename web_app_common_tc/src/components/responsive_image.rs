use topcoat::{
    Result,
    view::{component, view},
};

#[component]
pub async fn responsive_image(
    src: String,
    alt: String,
    class: Option<String>,
) -> Result {
    let css_class = class.unwrap_or_else(|| "object-cover w-full h-full".to_string());
    
    // Check if the image is a GCS asset URL that supports resolution variants
    let is_gcs_url = src.contains("storage.googleapis.com") || src.starts_with("https://storage.cloud.google.com");
    let fallback_src = src.clone();
    let alt_text = alt.clone();

    view! {
        if is_gcs_url {
            <picture>
                <source
                    media="(min-width: 1024px)"
                    srcset=(format!("{}_1920.webp", src))
                    type="image/webp"
                />
                <source
                    media="(min-width: 640px)"
                    srcset=(format!("{}_1024.webp", src))
                    type="image/webp"
                />
                <source
                    srcset=(format!("{}_640.webp", src))
                    type="image/webp"
                />
                <img
                    src=(fallback_src)
                    alt=(alt_text)
                    class=(css_class)
                    loading="lazy"
                />
            </picture>
        } else {
            <img
                src=(fallback_src)
                alt=(alt_text)
                class=(css_class)
                loading="lazy"
            />
        }
    }
}
