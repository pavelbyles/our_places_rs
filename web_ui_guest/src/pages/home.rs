use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::{View, view},
};
use web_client::{client::ListingSearchParams, get_api_client};
use web_ui_common::assets::HERO_IMAGE;
use web_ui_common::components::domain::{listing_card::ListingSummary, villa_card::villa_carousel};

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
    let field = "flex flex-col gap-1 px-4 py-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "h-9 bg-transparent text-sm font-normal tracking-normal text-foreground normal-case outline-none";
    Ok(view! {
        <section class="grid gap-6 pb-10 md:grid-cols-2 md:items-end">
            <h1 class="font-serif text-[clamp(48px,7vw,96px)] leading-[1.02] font-light tracking-[-0.025em]">"Jamaica's Finest Escapes"</h1>
            <p class="text-lg text-muted-foreground">"Curated private villas with dedicated butler service, from the cliffs of Negril to the hills above Ocho Rios."</p>
        </section>
        <section class="relative">
            <img src=(HERO_IMAGE) alt="A villa terrace overlooking the Caribbean Sea" class="h-[560px] w-full rounded-[var(--radius-surface,16px)] bg-photo-placeholder object-cover" />
            <form method="get" action="/listings" role="search" aria-label="Search villas" class="absolute inset-x-4 bottom-4 grid items-center divide-divider rounded-[var(--radius-surface,16px)] bg-card shadow-xs md:inset-x-8 md:bottom-8 md:grid-cols-[2fr_1fr_1fr_1fr_auto] md:divide-x">
                <label class=(field)>"Where"<input name="q" placeholder="Ocho Rios, Negril…" class=(input) /></label>
                <label class=(field)>"Check in"<input type="date" name="check_in" class=(input) /></label>
                <label class=(field)>"Check out"<input type="date" name="check_out" class=(input) /></label>
                <label class=(field)>"Guests"<input type="number" name="guests" min="1" value="2" class=(input) /></label>
                <div class="p-2"><button type="submit" class="h-11 w-full rounded-[var(--radius-control,10px)] bg-primary px-6 text-[15px] font-medium text-primary-foreground">"Search villas"</button></div>
            </form>
        </section>

        <section class="mt-28 space-y-8" aria-labelledby="season-heading">
            <div class="flex items-end justify-between">
                <h2 id="season-heading" class="font-serif text-[40px] tracking-[-0.015em]">"This season's stays"</h2>
                <a href="/listings" class="text-sm font-medium">"View all villas →"</a>
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
