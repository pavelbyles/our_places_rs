//! Room-by-room suites on the listing page (spec-122 §2.3): a photo with a
//! slow hover zoom, the suite name, bed setup, guests and bath, and chips for
//! its view and features.

use common::listing_details::SuiteItem;
use topcoat::context::Cx;
use topcoat::{
    Result,
    icon::iconify::iconify_icon,
    view::{View, attributes, component, view},
};

use crate::components::domain::responsive_image::responsive_image;
use crate::components::svg_icon::svg_icon;

/// Suites shown before "Show all".
pub const SUITES_SHOWN: usize = 4;

/// "2 guests" / "1 guest".
fn guests(n: i32) -> String {
    match n {
        1 => "1 guest".to_string(),
        n => format!("{} guests", n.max(0)),
    }
}

/// One suite card. `photo` is the suite's image URL, resolved from the
/// listing's own images.
#[component]
pub async fn suite_card(
    cx: &Cx,
    /// The suite.
    suite: SuiteItem,
    /// The suite's photo, if it has one.
    photo: Option<String>,
) -> Result<impl View> {
    let SuiteItem {
        name,
        bed_configuration,
        max_guests,
        features,
        en_suite,
        ..
    } = suite;
    let bath = en_suite
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| "Shared bathroom".to_string());
    let alt = name.clone();
    Ok(view! {
        <article
            data-suite=""
            class="group grid grid-rows-[auto_1fr] overflow-hidden rounded-[var(--radius-surface,16px)] border border-border bg-card text-card-foreground"
        >
            <div class="relative aspect-[3/2] overflow-hidden bg-photo-placeholder">
                if let Some(src) = photo {
                    responsive_image(
                        src: src,
                        alt: alt,
                        sizes: Some("(min-width: 1024px) 400px, (min-width: 640px) 45vw, 92vw".to_string()),
                        attrs: attributes! {
                            class="transition-transform duration-700 ease-out group-hover:scale-105 motion-reduce:transition-none"
                        }
                    )
                }
            </div>
            <div class="grid content-start gap-3 px-5 pt-5 pb-6">
                <h3 class="font-serif text-[22px] leading-tight">(name)</h3>
                <p class="flex flex-wrap gap-x-4 gap-y-1.5 text-[13px] text-muted-foreground">
                    <span class="inline-flex items-center gap-1.5">
                        svg_icon(data: iconify_icon!("lucide:bed-double"), attrs: attributes! { class="size-4" })
                        (bed_configuration)
                    </span>
                    <span class="inline-flex items-center gap-1.5">
                        svg_icon(data: iconify_icon!("lucide:users"), attrs: attributes! { class="size-4" })
                        (guests(max_guests))
                    </span>
                    <span class="inline-flex items-center gap-1.5">
                        svg_icon(data: iconify_icon!("lucide:bath"), attrs: attributes! { class="size-4" })
                        (bath)
                    </span>
                </p>
                if !features.is_empty() {
                    <ul class="flex flex-wrap gap-1.5">
                        for feature in features {
                            <li class="rounded-full bg-chip px-2.5 py-0.5 text-xs">(feature)</li>
                        }
                    </ul>
                }
            </div>
        </article>
    })
}

/// The suites grid: the first four, then the rest behind "Show all N
/// suites" (a `<details>`, so it works without JavaScript).
#[component]
pub async fn suites_grid(
    cx: &Cx,
    /// Suites with their resolved photos, in host order.
    suites: Vec<(SuiteItem, Option<String>)>,
) -> Result<impl View> {
    let total = suites.len();
    let mut first = suites;
    let rest = first.split_off(first.len().min(SUITES_SHOWN));
    Ok(view! {
        <div data-testid="suites" class="grid gap-6 sm:grid-cols-2">
            for (suite, photo) in first {
                suite_card(suite: suite, photo: photo)
            }
        </div>
        if !rest.is_empty() {
            <details class="group/more">
                <summary
                    class="w-fit cursor-pointer list-none text-sm text-primary underline underline-offset-4 group-open/more:hidden"
                >
                    (format!("Show all {total} suites"))
                </summary>
                <div class="mt-6 grid gap-6 sm:grid-cols-2">
                    for (suite, photo) in rest {
                        suite_card(suite: suite, photo: photo)
                    }
                </div>
            </details>
        }
    })
}
