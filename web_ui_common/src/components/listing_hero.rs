//! The listing page's full-bleed hero (spec-122 §2.1): the first photo with
//! a gradient, a location pill, the serif title, the rating and a button to
//! the photo gallery. Without photos it is a plain title band.

use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, attributes, component, view},
};

use topcoat::icon::iconify::iconify_icon;

use crate::components::domain::responsive_image::responsive_image;
use crate::components::svg_icon::svg_icon;

/// "4.9 · 18 reviews", "4.9", or "New listing" when nobody has rated it.
pub fn hero_rating_text(rating: Option<f64>, review_count: Option<i64>) -> String {
    match (rating, review_count) {
        (Some(r), Some(1)) => format!("{r:.1} · 1 review"),
        (Some(r), Some(n)) if n > 1 => format!("{r:.1} · {n} reviews"),
        (Some(r), _) => format!("{r:.1}"),
        (None, _) => "New listing".to_string(),
    }
}

/// The hero. Place it first in the page so it meets the header.
#[component]
pub async fn listing_hero(
    cx: &Cx,
    /// Listing name (the page's `<h1>`).
    name: String,
    /// "City, Country".
    location: String,
    /// Overall rating, if any.
    rating: Option<f64>,
    /// Number of reviews, if known.
    review_count: Option<i64>,
    /// The first photo, if any.
    photo: Option<String>,
    /// How many photos the gallery holds.
    photo_count: usize,
    /// Alt text for the photo (its name, or "{listing}, photo 1").
    #[default]
    photo_alt: String,
) -> Result<impl View> {
    let rating_text = hero_rating_text(rating, review_count);
    let photos_label = match photo_count {
        1 => "View the photo".to_string(),
        n => format!("View all {n} photos"),
    };
    let (tone, pill, button) = if photo.is_some() {
        (
            "text-white",
            "border-white/60",
            "border-white/50 bg-white/15 text-white hover:bg-white/25",
        )
    } else {
        (
            "bg-card text-card-foreground",
            "border-border",
            "border-border text-foreground hover:bg-chip",
        )
    };
    Ok(view! {
        <section
            data-testid="listing-hero"
            aria-labelledby="listing-title"
            class=(format!("full-bleed relative isolate overflow-hidden {tone}"))
        >
            if let Some(src) = photo {
                <div class="absolute inset-0 -z-10 bg-photo-placeholder">
                    responsive_image(
                        src: src,
                        alt: photo_alt,
                        eager: true,
                        attrs: attributes! { data-hero-photo="" }
                    )
                </div>
                <div
                    aria-hidden="true"
                    class="absolute inset-0 -z-10 bg-linear-to-b from-black/5 via-black/15 to-black/75"
                ></div>
            }
            <div
                class="bleed-inner flex min-h-[clamp(380px,62vh,600px)] flex-col justify-end gap-3.5 pt-16 pb-10"
            >
                <span
                    class=(format!(
                        "w-fit rounded-full border px-3.5 py-1.5 text-xs tracking-[0.2em] uppercase backdrop-blur-sm {pill}",
                    ))
                >
                    (location)
                </span>
                <h1
                    id="listing-title"
                    class="font-serif text-[clamp(40px,6vw,76px)] leading-[1.05] font-light text-balance"
                >
                    (name)
                </h1>
                <div class="flex flex-wrap items-center gap-x-5 gap-y-2 text-sm">
                    <span data-testid="hero-rating" class="inline-flex items-center gap-1.5">
                        if rating.is_some() {
                            svg_icon(
                                data: iconify_icon!("lucide:star"),
                                attrs: attributes! { class="size-4 fill-current" }
                            )
                        }
                        (rating_text)
                    </span>
                    if photo_count > 0 {
                        <button
                            type="button"
                            data-dialog-open="gallery-dialog"
                            class=(format!(
                                "h-10 rounded-full border px-4 backdrop-blur-sm transition-colors {button}",
                            ))
                        >
                            (photos_label)
                        </button>
                    }
                </div>
            </div>
        </section>
    })
}
