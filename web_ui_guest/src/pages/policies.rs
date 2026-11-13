//! Policy pages linked from the footer: terms, cancellation, privacy and
//! house rules. Their text is a DRAFT PLACEHOLDER (spec-122 decision 2):
//! each page says so on screen until the owner supplies the real wording.

use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::{View, component, view},
};

/// The notice every placeholder policy page shows.
pub const DRAFT_NOTICE: &str = "Draft \u{2014} placeholder text. This page will be replaced with our final policy before launch. Contact us with any questions in the meantime.";

/// A policy page: title, a draft notice, and (heading, paragraph) sections.
#[component]
async fn policy_page(
    cx: &Cx,
    /// Page heading.
    title: &'static str,
    /// One-line summary under the heading.
    summary: &'static str,
    /// Sections as (heading, text).
    sections: Vec<(&'static str, &'static str)>,
) -> Result<impl View> {
    Ok(view! {
        <article class="mx-auto max-w-3xl space-y-8">
            <header class="space-y-3">
                <p class="text-xs font-medium tracking-[0.22em] text-primary uppercase">"Policies"</p>
                <h1 class="font-serif text-[clamp(36px,5vw,56px)] leading-tight">(title)</h1>
                <p class="text-lg text-muted-foreground">(summary)</p>
            </header>
            <p
                data-testid="policy-draft-notice"
                role="note"
                class="rounded-lg border border-border bg-chip px-4 py-3 text-sm"
            >
                (DRAFT_NOTICE)
            </p>
            for (heading, text) in sections {
                <section class="space-y-2">
                    <h2 class="font-serif text-2xl">(heading)</h2>
                    <p class="leading-relaxed text-muted-foreground">(text)</p>
                </section>
            }
        </article>
    })
}

/// Terms of booking and use.
#[page("/terms")]
pub async fn terms_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        policy_page(
            title: "Terms and conditions",
            summary: "The terms that apply when you book a stay with Our Places.",
            sections: vec![
                ("Bookings", "Placeholder: how a booking is made, confirmed and paid for."),
                ("Your stay", "Placeholder: guest numbers, check-in and check-out, and use of the property."),
                ("Liability", "Placeholder: responsibilities of guests and hosts."),
            ]
        )
    })
}

/// Cancellation and refunds.
#[page("/cancellation-policy")]
pub async fn cancellation_policy_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        policy_page(
            title: "Cancellation policy",
            summary: "What happens if you need to cancel or change a booking.",
            sections: vec![
                ("Cancelling", "Placeholder: refund amounts by how far ahead you cancel."),
                ("Changing dates", "Placeholder: how date and guest changes are requested and priced."),
                ("Security deposits", "Placeholder: when deposits are taken and released."),
            ]
        )
    })
}

/// Privacy.
#[page("/privacy-policy")]
pub async fn privacy_policy_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        policy_page(
            title: "Privacy policy",
            summary: "How we collect, use and protect your information.",
            sections: vec![
                ("What we collect", "Placeholder: the details we ask for when you book or enquire."),
                ("How we use it", "Placeholder: bookings, messages and receipts."),
                ("Your choices", "Placeholder: how to see, change or delete your information."),
            ]
        )
    })
}

/// House rules.
#[page("/house-rules")]
pub async fn house_rules_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        policy_page(
            title: "House rules",
            summary: "The rules that apply at every Our Places property. Each listing shows its own rules too.",
            sections: vec![
                ("Respect the house", "Placeholder: smoking, pets and care of the property."),
                ("Quiet hours and events", "Placeholder: quiet hours and how to ask about events."),
                ("Guests", "Placeholder: who may stay and visit."),
            ]
        )
    })
}
