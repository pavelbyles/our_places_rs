//! Host-configured amenity groups on the listing page (spec-122 §2.4): one
//! card per group with an icon, a serif title and a checklist.

use common::listing_details::{AmenityGroup, AmenityIcon};
use topcoat::context::Cx;
use topcoat::{
    Result,
    icon::{IconData, iconify::iconify_icon},
    view::{View, attributes, component, view},
};

use crate::components::svg_icon::svg_icon;

/// The Lucide icon drawn for each amenity group icon choice.
pub fn amenity_icon(icon: AmenityIcon) -> IconData {
    match icon {
        AmenityIcon::Home => iconify_icon!("lucide:house"),
        AmenityIcon::Waves => iconify_icon!("lucide:waves"),
        AmenityIcon::Heart => iconify_icon!("lucide:heart"),
        AmenityIcon::Family => iconify_icon!("lucide:users"),
        AmenityIcon::Sun => iconify_icon!("lucide:sun"),
        AmenityIcon::Utensils => iconify_icon!("lucide:utensils"),
        AmenityIcon::Sparkles => iconify_icon!("lucide:sparkles"),
        AmenityIcon::Shield => iconify_icon!("lucide:shield"),
        AmenityIcon::Car => iconify_icon!("lucide:car"),
        AmenityIcon::Star => iconify_icon!("lucide:star"),
    }
}

/// A list with a check mark before each item (amenities, included staff).
#[component]
pub async fn check_list(
    cx: &Cx,
    /// The items.
    items: Vec<String>,
) -> Result<impl View> {
    Ok(view! {
        <ul class="grid gap-2 text-sm">
            for item in items {
                <li class="flex items-start gap-2.5">
                    svg_icon(
                        data: iconify_icon!("lucide:check"),
                        attrs: attributes! { class="mt-0.5 size-4 shrink-0 text-primary" }
                    )
                    <span>(item)</span>
                </li>
            }
        </ul>
    })
}

/// A grid of amenity group cards. Renders nothing for an empty list.
#[component]
pub async fn amenity_groups(
    cx: &Cx,
    /// Groups in display order (already without empty groups).
    groups: Vec<AmenityGroup>,
) -> Result<impl View> {
    Ok(view! {
        if !groups.is_empty() {
            <div
                data-testid="amenity-groups"
                class="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-5"
            >
                for group in groups {
                    <div
                        data-amenity-group=""
                        class="grid content-start gap-3.5 rounded-[var(--radius-surface,16px)] border border-border bg-card p-5 text-card-foreground"
                    >
                        <h3 class="flex items-center gap-2.5 font-serif text-xl">
                            svg_icon(
                                data: amenity_icon(group.icon),
                                attrs: attributes! { class="size-[22px] text-primary" }
                            )
                            (group.title)
                        </h3>
                        check_list(items: group.items)
                    </div>
                }
            </div>
        }
    })
}
