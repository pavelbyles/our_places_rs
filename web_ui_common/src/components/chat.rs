//! Guest–host message threads.

use topcoat::{
    Result,
    view::{Attributes, Child, View, class, component, view},
};

/// A scrolling message log. Give it the `id` that new messages are swapped
/// into (`#messages-list` / `#admin-messages-list`).
#[component]
pub async fn chat_thread(
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
    /// The [`chat_bubble`]s.
    #[default]
    child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            role="log"
            aria-live="polite"
            class=(class!("flex max-h-[60vh] flex-col gap-4 overflow-y-auto", attrs.remove("class")))
            (attrs)
        >(child)</div>
    })
}

/// One message. `mine` aligns it to the end and uses the primary fill.
#[component]
pub async fn chat_bubble(
    /// Sender display name.
    sender: String,
    /// Formatted send time.
    sent_at: String,
    /// Message text, rendered as text (line breaks kept).
    text: String,
    /// Whether the viewer sent it.
    #[default]
    mine: bool,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let align = if mine { "items-end" } else { "items-start" };
    let fill = if mine {
        "bg-primary text-primary-foreground"
    } else {
        "bg-card text-card-foreground border border-border"
    };
    Ok(view! {
        <div class=(class!("flex flex-col gap-1", align, attrs.remove("class"))) (attrs)>
            <p class="text-xs text-muted-foreground">(sender) " · " (sent_at)</p>
            <p class=(class!("max-w-[75%] rounded-2xl px-4 py-2 text-sm whitespace-pre-wrap", fill))>(text)</p>
        </div>
    })
}
