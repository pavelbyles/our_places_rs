//! A labelled metric for dashboards.

use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

/// A metric tile: label, prominent value and an optional hint.
#[component]
pub async fn stat_tile(
    /// What the number measures.
    label: String,
    /// The formatted value.
    value: String,
    /// Optional supporting text, e.g. a period or change.
    #[default]
    hint: Option<String>,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!("rounded-xl border border-border bg-card p-4 text-card-foreground", attrs.remove("class")))
            (attrs)
        >
            <p class="text-xs font-medium tracking-wide text-muted-foreground uppercase">(label)</p>
            <p class="mt-1 text-2xl font-semibold tabular-nums">(value)</p>
            if let Some(hint) = hint {
                <p class="mt-1 text-xs text-muted-foreground">(hint)</p>
            }
        </div>
    })
}
