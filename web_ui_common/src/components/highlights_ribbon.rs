//! The listing page's full-width ribbon of host-entered key items
//! (spec-122 §2.2): an icon, a large serif value and a tracked label each.

use common::listing_details::{Ribbon, RibbonIcon};
use topcoat::context::Cx;
use topcoat::{
    Result,
    icon::{IconData, iconify::iconify_icon},
    view::{Length, View, attributes, component, view},
};

use crate::components::svg_icon::svg_icon;

/// The Lucide icon drawn for each ribbon icon choice.
pub fn ribbon_icon(icon: RibbonIcon) -> IconData {
    match icon {
        RibbonIcon::Bed => iconify_icon!("lucide:bed-double"),
        RibbonIcon::Bath => iconify_icon!("lucide:bath"),
        RibbonIcon::Guests => iconify_icon!("lucide:users"),
        RibbonIcon::Pool => iconify_icon!("lucide:waves"),
        RibbonIcon::Beach => iconify_icon!("lucide:house"),
        RibbonIcon::Ocean => iconify_icon!("lucide:sunrise"),
        RibbonIcon::Chef => iconify_icon!("lucide:chef-hat"),
        RibbonIcon::Butler => iconify_icon!("lucide:concierge-bell"),
        RibbonIcon::Security => iconify_icon!("lucide:shield"),
        RibbonIcon::Wifi => iconify_icon!("lucide:wifi"),
        RibbonIcon::Solar => iconify_icon!("lucide:sun"),
        RibbonIcon::Car => iconify_icon!("lucide:car"),
        RibbonIcon::Gym => iconify_icon!("lucide:dumbbell"),
        RibbonIcon::Star => iconify_icon!("lucide:star"),
    }
}

/// Grid columns for `n` items: 2 on phones, at most 3 on tablets, one row
/// of `n` from 860px. Literal strings so Tailwind finds every class.
pub fn ribbon_columns(n: usize) -> &'static str {
    match n {
        0..=3 => "grid-cols-2 min-[600px]:grid-cols-3",
        4 => "grid-cols-2 min-[860px]:grid-cols-4",
        5 => "grid-cols-2 min-[600px]:grid-cols-3 min-[860px]:grid-cols-5",
        _ => "grid-cols-2 min-[600px]:grid-cols-3 min-[860px]:grid-cols-6",
    }
}

/// The full-bleed highlights ribbon. Place it as a direct child of the
/// shell's `.page-grid` so it spans the window.
#[component]
pub async fn highlights_ribbon(
    cx: &Cx,
    /// The items to show (already validated, or the facts fallback).
    ribbon: Ribbon,
) -> Result<impl View> {
    let columns = ribbon_columns(ribbon.items().len());
    let items = ribbon.items().to_vec();
    Ok(view! {
        <section
            data-testid="highlights-ribbon"
            aria-label="Villa at a glance"
            class="full-bleed bg-primary text-primary-foreground"
        >
            <div class="bleed-inner py-12 sm:py-16 lg:py-[72px]">
                <ul
                    class=(format!(
                        "grid gap-x-6 gap-y-10 max-[599px]:[&>li:last-child:nth-child(odd)]:col-span-2 {columns}",
                    ))
                >
                    for item in items {
                        <li class="grid justify-items-center gap-2.5 text-center">
                            svg_icon(
                                data: ribbon_icon(item.icon),
                                size: Length::px(34.0),
                                attrs: attributes! { class="text-primary-foreground" }
                            )
                            <span
                                class="font-serif text-[clamp(40px,4.6vw,64px)] font-light leading-none"
                            >
                                (item.value)
                            </span>
                            <span
                                class="text-xs tracking-[0.32em] text-primary-foreground/75 uppercase"
                            >
                                (item.label)
                            </span>
                        </li>
                    }
                </ul>
            </div>
        </section>
    })
}
