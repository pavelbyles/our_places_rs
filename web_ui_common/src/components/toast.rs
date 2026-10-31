//! Toast notifications: a polite live region that `ui.js` fills from
//! `HX-Trigger: {"toast": …}` events, and a server-rendered toast.

use topcoat::{
    Result,
    view::{Attributes, StaticClass, View, class, component, view},
};

/// Tone of a [`toast`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastVariant {
    /// Neutral information.
    #[default]
    Info,
    /// A completed action.
    Success,
    /// A failed action.
    Error,
}

impl ToastVariant {
    /// The `data-variant` value, matching what `ui.js` writes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Error => "error",
        }
    }
}

const TOAST: StaticClass = class!(
    "rounded-[var(--radius-control,10px)] border border-border bg-popover px-4 py-3 \
     text-sm text-popover-foreground shadow-sm data-[variant=error]:border-destructive \
     data-[variant=error]:text-destructive",
);

/// The fixed live region toasts are appended to. Render it once per page.
#[component]
pub async fn toast_region(
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <div
            id="toast-region"
            aria-live="polite"
            class=(class!("fixed right-4 bottom-4 z-50 flex flex-col gap-2", attrs.remove("class")))
            (attrs)
        ></div>
    })
}

/// A single server-rendered toast, e.g. after a full-page form post.
#[component]
pub async fn toast(
    /// The message, rendered as text.
    message: String,
    /// Tone of the toast.
    #[default]
    variant: ToastVariant,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <div
            role="status"
            data-variant=(variant.as_str())
            class=(class!(TOAST, attrs.remove("class")))
            (attrs)
        >(message)</div>
    })
}
