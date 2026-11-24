//! The listing page's sticky in-page nav (spec-122 §2.1): anchor links to
//! the sections the listing has, plus the nightly price and a "Check
//! availability" link on wide screens. Plain links, so it works without
//! JavaScript; `ui.js` marks the section in view.

use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};

/// An outlined pill link or button (spec-122 §2.1 buttons).
pub const PILL: &str = "inline-flex h-11 items-center justify-center gap-2 rounded-full border border-primary px-6 text-xs font-medium tracking-[0.16em] text-primary uppercase no-underline transition-colors hover:bg-primary hover:text-primary-foreground";

/// The smaller outlined pill used in bars and cards.
pub const PILL_SM: &str = "inline-flex h-9 items-center justify-center rounded-full border border-primary px-4 text-[11px] font-medium tracking-[0.16em] text-primary uppercase no-underline transition-colors hover:bg-primary hover:text-primary-foreground";

/// The sticky section nav. Place it first inside a full-bleed block that
/// holds the rest of the page, so it sticks for that block's height.
#[component]
pub async fn section_nav(
    cx: &Cx,
    /// `(section id, label)` for each section the page shows, in page order.
    links: Vec<(&'static str, &'static str)>,
    /// The nightly price text, e.g. `US$650`.
    price: String,
) -> Result<impl View> {
    Ok(view! {
        <nav
            aria-label="On this page"
            data-section-nav=""
            class="sticky top-0 z-20 border-b border-border bg-card/95 text-card-foreground backdrop-blur"
        >
            <div class="bleed-inner flex items-center justify-between gap-6">
                <ul class="flex min-w-0 gap-6 overflow-x-auto [scrollbar-width:none]">
                    for (id, label) in links {
                        <li class="shrink-0">
                            <a
                                href=(format!("#{id}"))
                                data-section-link=(id)
                                class="block border-b-2 border-transparent py-4 text-[13px] tracking-[0.12em] whitespace-nowrap text-muted-foreground uppercase no-underline hover:text-foreground aria-[current=true]:border-primary aria-[current=true]:text-foreground"
                            >
                                (label)
                            </a>
                        </li>
                    }
                </ul>
                <div class="hidden shrink-0 items-center gap-4 text-sm lg:flex">
                    <span>
                        <strong class="font-medium">(price)</strong>
                        <span class="text-muted-foreground">" / night"</span>
                    </span>
                    <a href="#book" class=(PILL_SM)>"Check availability"</a>
                </div>
            </div>
        </nav>
    })
}
