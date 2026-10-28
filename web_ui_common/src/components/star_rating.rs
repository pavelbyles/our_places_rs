//! Star ratings: a compact display and an accessible 1–5 input.

use topcoat::{
    Result,
    icon::{icon, iconify::iconify_icon},
    view::{Attributes, View, attributes, class, component, view},
};

/// An average rating such as "★ 4.7 (12 reviews)", or "New" when there is
/// none yet.
#[component]
pub async fn star_rating(
    /// Average rating out of 5, if any reviews exist.
    rating: Option<f64>,
    /// Number of reviews behind the average.
    #[default]
    review_count: Option<i64>,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let root = class!(
        "inline-flex items-center gap-1.5 text-sm",
        attrs.remove("class")
    );
    Ok(view! {
        if let Some(r) = rating {
            <span class=(root) (attrs)>
                <span role="img" aria-label=(format!("Rated {r:.1} out of 5")) class="inline-flex items-center gap-1 font-medium">
                    icon(data: iconify_icon!("lucide:star"), attrs: attributes! { class="size-4 fill-current text-primary" aria-hidden="true" })
                    (format!("{r:.1}"))
                </span>
                if let Some(count) = review_count {
                    <span class="text-muted-foreground">(format!("({count} reviews)"))</span>
                }
            </span>
        } else {
            <span class=(root) (attrs)>
                <span class="rounded-full border border-border px-2 py-0.5 text-xs text-muted-foreground">"New"</span>
            </span>
        }
    })
}

/// A labelled 1–5 star choice. Submits `name` with the chosen value.
#[component]
pub async fn star_rating_input(
    /// Form field name, e.g. `cleanliness_rating`.
    name: String,
    /// Visible label, e.g. "Cleanliness".
    label: String,
    /// Pre-selected value (1–5).
    #[default]
    value: Option<u8>,
    /// Extra HTML attributes for the `<fieldset>`; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <fieldset class=(class!("space-y-2", attrs.remove("class"))) (attrs)>
            <legend class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">(label.clone())</legend>
            <div class="flex gap-1">
                for n in 1u8..=5 {
                    let id = format!("{name}-{n}");
                    <label for=(id.clone()) class="cursor-pointer">
                        <input
                            type="radio"
                            id=(id)
                            name=(name.clone())
                            value=(n.to_string())
                            required=(true)
                            checked=(value == Some(n))
                            class="peer sr-only"
                        />
                        <span
                            class="inline-flex size-11 items-center justify-center rounded-full border border-border text-muted-foreground peer-checked:border-primary peer-checked:bg-primary peer-checked:text-primary-foreground peer-focus-visible:ring-2 peer-focus-visible:ring-ring"
                        >
                            (n)
                            <span class="sr-only">(format!(" stars for {label}"))</span>
                        </span>
                    </label>
                }
            </div>
        </fieldset>
    })
}
