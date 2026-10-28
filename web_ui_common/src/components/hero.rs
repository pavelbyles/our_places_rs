//! The guest home hero: serif headline and lead, then a wide photo with the
//! search bar laid over it (docs/design/guest-theme.md §4).

use topcoat::{
    Result,
    view::{Attributes, Child, View, class, component, view},
};

/// Hero section. The search form goes in as children and is overlaid on the
/// bottom of the photo.
#[component]
pub async fn hero(
    /// Display headline (the page's `h1`).
    headline: String,
    /// Lead paragraph.
    lead: String,
    /// Landscape photo URL.
    image_src: String,
    /// Alternative text for the photo.
    image_alt: String,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
    /// Overlaid content, typically the search form.
    #[default]
    child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <section class=(class!("mx-auto w-full max-w-[1240px] px-6", attrs.remove("class"))) (attrs)>
            <div class="grid gap-6 py-12 md:grid-cols-2 md:items-end">
                <h1 class="font-serif text-[clamp(48px,7vw,96px)] leading-[1.02] font-light tracking-[-0.025em]">
                    (headline)
                </h1>
                <p class="text-lg text-muted-foreground">(lead)</p>
            </div>
            <div class="relative">
                <img
                    src=(image_src)
                    alt=(image_alt)
                    class="h-[560px] w-full rounded-[var(--radius-surface,16px)] bg-photo-placeholder object-cover"
                />
                <div class="absolute inset-x-4 bottom-4 md:inset-x-8 md:bottom-8">(child)</div>
            </div>
        </section>
    })
}
