use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::{View, component, view},
};
use web_client::{client::ListingSearchParams, get_api_client};
use web_ui_common::assets::HERO_IMAGE;
use web_ui_common::components::domain::{listing_card::ListingSummary, villa_card::villa_carousel};

use crate::support::{DEFAULT_GUESTS, display_currency, jamaica_today};

/// Trust strip items (docs/design/guest-theme.md §4).
const PERKS: [(&str, &str); 3] = [
    (
        "Dedicated Butler & Staff",
        "Every villa comes with a trained team, from chef to concierge.",
    ),
    (
        "Verified reviews",
        "Only guests who stayed can review, so every rating is real.",
    ),
    (
        "Prices in your currency",
        "See every stay in USD, JMD, EUR, GBP or CAD before you book.",
    ),
];

/// The hero's search form, which leads to `/listings`.
#[component]
async fn home_search_form(
    cx: &Cx,
    /// Today, the earliest date to offer.
    today: String,
    /// Display currency, kept across the search.
    currency: Option<String>,
) -> Result<impl View> {
    let field = "flex flex-col gap-1 px-4 py-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "h-9 bg-transparent text-sm font-normal tracking-normal text-foreground normal-case outline-none";
    Ok(view! {
        <form method="get" action="/listings" role="search" aria-label="Search villas" class="absolute inset-x-4 bottom-4 grid items-center divide-divider rounded-[var(--radius-surface,16px)] bg-card shadow-xs md:inset-x-8 md:bottom-8 md:grid-cols-[2fr_1fr_1fr_1fr_auto] md:divide-x">
            if let Some(currency) = currency {
                <input type="hidden" name="currency" value=(currency) />
            }
            <label class=(field)>"Where"<input name="q" placeholder="Runaway Bay, Kingston…" class=(input) /></label>
            <label class=(field)>"Check in"<input type="text" data-date-picker="" placeholder="yyyy-mm-dd" pattern="\\d{4}-\\d{2}-\\d{2}" name="check_in" min=(today.clone()) class=(input) /></label>
            <label class=(field)>"Check out"<input type="text" data-date-picker="" placeholder="yyyy-mm-dd" pattern="\\d{4}-\\d{2}-\\d{2}" name="check_out" min=(today) class=(input) /></label>
            <label class=(field)>"Guests"<input type="number" name="guests" min="1" value=(DEFAULT_GUESTS.to_string()) class=(input) /></label>
            <div class="p-2"><button type="submit" class="h-11 w-full rounded-[var(--radius-control,10px)] bg-primary px-6 text-[15px] font-medium text-primary-foreground">"Search villas"</button></div>
        </form>
    })
}

/// Home: hero with search, this season's stays, and the trust strip.
#[page("/")]
pub async fn home_page(cx: &Cx) -> Result<impl View> {
    let featured: Vec<ListingSummary> = get_api_client(cx)
        .search_listings(ListingSearchParams {
            per_page: Some(8),
            ..Default::default()
        })
        .await
        .unwrap_or_default()
        .iter()
        .map(ListingSummary::from)
        .collect();
    let today = jamaica_today().to_string();
    let currency = display_currency(cx).await;
    Ok(view! {
        <section class="grid gap-6 pb-10 md:grid-cols-2 md:items-end">
            <h1 class="font-serif text-[clamp(48px,7vw,96px)] leading-[1.02] font-light tracking-[-0.025em]">"Jamaica's Finest Escapes"</h1>
            <p class="text-lg text-muted-foreground">"Hand-picked private villas and apartments in Runaway Bay and Kingston."</p>
        </section>
        <section class="relative">
            <img src=(HERO_IMAGE) alt="A villa terrace overlooking the Caribbean Sea" class="h-[560px] w-full rounded-[var(--radius-surface,16px)] bg-photo-placeholder object-cover" />
            home_search_form(today: today, currency: currency)
        </section>

        <section class="mt-28 space-y-8" aria-labelledby="season-heading">
            <div class="flex items-end justify-between">
                <h2 id="season-heading" class="font-serif text-[40px] tracking-[-0.015em]">"This season's stays"</h2>
                <a href="/listings" class="text-sm font-medium">"View all listings →"</a>
            </div>
            villa_carousel(listings: featured, track_id: "featured-villas".to_string(), label: "This season's stays".to_string())
        </section>

        <section class="mt-32 border-t border-border pt-12" aria-labelledby="trust-heading">
            <h2 id="trust-heading" class="font-serif text-[32px]">"Booking, without the noise."</h2>
            <ul class="mt-8 grid grid-cols-[repeat(auto-fit,minmax(260px,1fr))] gap-8">
                for (title, body) in PERKS {
                    <li class="space-y-2">
                        <h3 class="font-medium">(title)</h3>
                        <p class="text-sm text-muted-foreground">(body)</p>
                    </li>
                }
            </ul>
        </section>
    })
}

#[cfg(test)]
mod default_guests_tests {
    use super::*;
    use topcoat::view::ViewExt;

    async fn form(currency: Option<&str>) -> String {
        let cx = &Cx::default();
        let currency = currency.map(str::to_string);
        let rendered =
            view! { home_search_form(today: "2030-06-15".to_string(), currency: currency) }
                .first()
                .await;
        match rendered {
            Ok(handle) => handle.render(cx),
            Err(e) => panic!("render failed: {e}"),
        }
    }

    #[tokio::test]
    async fn the_form_defaults_to_one_guest_and_todays_date() {
        let out = form(None).await;
        assert!(out.contains(r#"name="guests" min="1" value="1""#), "{out}");
        assert!(out.contains(r#"name="check_in" min="2030-06-15""#), "{out}");
        assert!(
            out.contains(r#"name="check_out" min="2030-06-15""#),
            "{out}"
        );
        assert!(!out.contains(r#"name="currency""#), "{out}");
    }

    #[tokio::test]
    async fn a_chosen_currency_survives_the_search() {
        let out = form(Some("JMD")).await;
        assert!(
            out.contains(r#"type="hidden" name="currency" value="JMD""#),
            "{out}"
        );
    }
}
