//! A numbered progress indicator for multi-step flows such as checkout.

use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

/// Ordered steps with the current one marked `aria-current="step"`.
///
/// `current` is zero-based; steps before it render as complete.
#[component]
pub async fn steps(
    /// Step labels, in order.
    labels: Vec<String>,
    /// Zero-based index of the current step.
    current: usize,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <ol class=(class!("flex flex-wrap items-center gap-3 text-sm", attrs.remove("class"))) (attrs)>
            for (i, label) in labels.into_iter().enumerate() {
                let state = if i < current { "complete" } else if i == current { "current" } else { "upcoming" };
                <li
                    data-state=(state)
                    aria-current=((i == current).then_some("step"))
                    class="flex items-center gap-2 text-muted-foreground data-[state=current]:font-medium data-[state=current]:text-foreground"
                >
                    <span
                        aria-hidden="true"
                        class="inline-flex size-6 items-center justify-center rounded-full border border-border text-xs"
                    >(i + 1)</span>
                    <span>(label)</span>
                </li>
            }
        </ol>
    })
}
