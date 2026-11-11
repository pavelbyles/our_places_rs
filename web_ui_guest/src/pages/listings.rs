use common::app_client::ListingSearchRefusal;
use common::models::ListingResponse;
use common::query::urlencode;
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{page, parse_query_params},
    view::{View, component, view},
};
use web_client::{client::ListingSearchParams, get_api_client};
use web_ui_common::components::domain::{
    listing_card::{ListingSummary, SearchLink},
    villa_card::villa_carousel,
};

use crate::support::{display_currency, jamaica_today};

/// Listings asked for on one page.
const SEARCH_PAGE_SIZE: u32 = 50;

/// A search, as the guest gave it. Everything stays text and goes to the API
/// unchanged, so a bad date gets the API's message and doesn't lose the rest.
#[derive(Debug, Deserialize, Default, Clone, PartialEq)]
struct SearchQuery {
    q: Option<String>,
    currency: Option<String>,
    /// A host's slug: only their listings.
    host: Option<String>,
    check_in: Option<String>,
    check_out: Option<String>,
    guests: Option<String>,
}

/// A value that was given, trimmed.
fn given(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

impl SearchQuery {
    fn host(&self) -> Option<String> {
        given(&self.host).map(|h| h.to_lowercase())
    }

    fn has_dates(&self) -> bool {
        given(&self.check_in).is_some() && given(&self.check_out).is_some()
    }

    /// What goes to the API. Only these fields do, whatever else is in the URL.
    fn params(&self, currency: Option<String>) -> ListingSearchParams {
        ListingSearchParams {
            q: given(&self.q),
            currency,
            host: self.host(),
            check_in: given(&self.check_in),
            check_out: given(&self.check_out),
            guests: given(&self.guests),
            per_page: Some(SEARCH_PAGE_SIZE),
            ..Default::default()
        }
    }

    fn link(&self, currency: Option<String>) -> SearchLink {
        SearchLink {
            check_in: given(&self.check_in),
            check_out: given(&self.check_out),
            guests: given(&self.guests),
            currency,
        }
    }

    /// This search without its host.
    fn all_listings_href(&self, currency: Option<String>) -> String {
        let text = given(&self.q).map(|q| format!("q={}", urlencode(&q)));
        let query = text
            .into_iter()
            .chain(self.link(currency).query())
            .collect::<Vec<_>>()
            .join("&");
        if query.is_empty() {
            "/listings".to_string()
        } else {
            format!("/listings?{query}")
        }
    }
}

/// What a search came to.
#[derive(Debug, Clone, PartialEq)]
enum SearchOutcome {
    /// The listings found, ready for their cards.
    Found(Vec<ListingSummary>),
    /// The API refused the search, with a message for the guest.
    Refused(String),
    /// Anything else went wrong.
    Failed,
}

const SEARCH_FAILED: &str = "We couldn't load listings. Please try again.";

/// Sorts a search result, and logs a failure. Only the error's outermost text is
/// logged, because the causes can include the URL, and so the search text.
fn classify(
    result: anyhow::Result<Vec<ListingResponse>>,
    query: &SearchQuery,
    currency: Option<String>,
) -> SearchOutcome {
    match result {
        Ok(listings) => {
            let search_query = query.link(currency).query();
            SearchOutcome::Found(
                listings
                    .iter()
                    .map(|listing| ListingSummary {
                        stay_searched: query.has_dates(),
                        search_query: search_query.clone(),
                        ..ListingSummary::from(listing)
                    })
                    .collect(),
            )
        }
        Err(e) => match e.downcast_ref::<ListingSearchRefusal>() {
            Some(refusal) => SearchOutcome::Refused(refusal.message.clone()),
            None => {
                tracing::error!(error = %e, "listing search failed");
                SearchOutcome::Failed
            }
        },
    }
}

/// The search form, filled in from the URL.
#[component]
async fn listing_search_form(
    cx: &Cx,
    /// The search so far.
    query: SearchQuery,
    /// Today, the earliest date to offer.
    today: String,
    /// Display currency, kept across searches.
    currency: Option<String>,
) -> Result<impl View> {
    let label =
        "flex flex-col gap-1 text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "h-11 rounded-full border border-border bg-card px-4 text-sm font-normal tracking-normal text-foreground normal-case";
    Ok(view! {
        <form id="listing-search-form" method="get" action="/listings" role="search" class="grid grid-cols-2 items-end gap-3 md:flex md:flex-wrap md:justify-end">
            if let Some(host) = query.host() {
                <input type="hidden" name="host" value=(host) />
            }
            if let Some(currency) = currency {
                <input type="hidden" name="currency" value=(currency) />
            }
            <div class="col-span-2 flex flex-col gap-1">
                <label for="listing-search" class=(label)>"Where"</label>
                <input id="listing-search" name="q" type="search" value=(given(&query.q).unwrap_or_default()) placeholder="Search by name or place..." class=(format!("{input} w-full md:w-64")) />
            </div>
            <label class=(label)>"Check in"<input id="search-check-in" type="text" data-date-picker="" placeholder="yyyy-mm-dd" pattern="\\d{4}-\\d{2}-\\d{2}" name="check_in" min=(today.clone()) value=(given(&query.check_in).unwrap_or_default()) class=(input) /></label>
            <label class=(label)>"Check out"<input id="search-check-out" type="text" data-date-picker="" placeholder="yyyy-mm-dd" pattern="\\d{4}-\\d{2}-\\d{2}" name="check_out" min=(today) value=(given(&query.check_out).unwrap_or_default()) class=(input) /></label>
            <label class=(label)>"Guests"<input id="search-guests" type="number" name="guests" min="1" value=(given(&query.guests).unwrap_or_default()) class=(format!("{input} w-full md:w-24")) /></label>
            <button type="submit" class="h-11 rounded-full bg-primary px-5 text-sm font-medium text-primary-foreground">"Search"</button>
        </form>
    })
}

/// What's below the form: cards, an empty state, or why there are none.
#[component]
async fn listings_results(
    cx: &Cx,
    /// What the search came to.
    outcome: SearchOutcome,
    /// What to say when nothing matched.
    empty: String,
) -> Result<impl View> {
    Ok(view! {
        match outcome {
            SearchOutcome::Found(listings) => {
                if listings.is_empty() {
                    <p class="py-10 text-center text-muted-foreground">(empty)</p>
                } else {
                    villa_carousel(listings: listings, track_id: "listings-grid".to_string(), label: "Available villas".to_string())
                }
            }
            SearchOutcome::Refused(message) => {
                <p id="search-refused" role="alert" class="py-10 text-center text-foreground">(message)</p>
            }
            SearchOutcome::Failed => {
                <p id="search-failed" role="alert" class="py-10 text-center text-foreground">(SEARCH_FAILED)</p>
            }
        }
    })
}

/// The "N stays" line, shown only when the search worked.
fn stays_line(outcome: &SearchOutcome) -> Option<String> {
    match outcome {
        SearchOutcome::Found(listings) => Some(format!("{} stays across Jamaica", listings.len())),
        SearchOutcome::Refused(_) | SearchOutcome::Failed => None,
    }
}

/// The empty state, which mentions dates when they were given.
fn empty_message(query: &SearchQuery) -> &'static str {
    if query.has_dates() {
        "No stays match those dates. Try different dates or fewer guests."
    } else if query.host().is_some() && given(&query.q).is_none() {
        "This host has no listings yet"
    } else {
        "No listings match your search criteria"
    }
}

/// Listings, optionally filtered by text, dates, guests and host (`?host={slug}`).
#[page("/listings")]
pub async fn listings_page(cx: &Cx) -> Result<impl View> {
    let query = parse_query_params::<SearchQuery>(cx).unwrap_or_default();
    let currency = display_currency(cx).await;
    let result = get_api_client(cx)
        .search_listings(query.params(currency.clone()))
        .await;
    let outcome = classify(result, &query, currency.clone());

    let host = query.host();
    // The host's display name comes from their listings; an unknown slug
    // simply finds none.
    let host_name = match (&host, &outcome) {
        (Some(slug), SearchOutcome::Found(listings)) => listings
            .iter()
            .filter_map(|l| l.host.as_ref())
            .find(|h| &h.slug == slug)
            .map(|h| h.name.clone()),
        _ => None,
    };
    let heading = match (&host, &host_name) {
        (Some(_), Some(name)) => format!("Listings by {name}"),
        (Some(_), None) => "Host listings".to_string(),
        (None, _) => "Explore All Listings".to_string(),
    };
    let summary = stays_line(&outcome);
    let empty = empty_message(&query).to_string();
    let all_listings = query.all_listings_href(currency.clone());
    let today = jamaica_today().to_string();
    Ok(view! {
        <div class="space-y-10">
            <div class="flex flex-col gap-6 md:flex-row md:items-end md:justify-between">
                <div class="space-y-2">
                    <h1 class="font-serif text-[clamp(36px,5vw,56px)] leading-tight">(heading)</h1>
                    <p class="text-muted-foreground">
                        if let Some(summary) = summary {
                            (summary)
                        }
                        if host.is_some() {
                            " · "<a id="clear-host-filter" href=(all_listings) class="font-medium text-foreground hover:underline underline-offset-4">"Show all listings"</a>
                        }
                    </p>
                </div>
                listing_search_form(query: query, today: today, currency: currency)
            </div>
            listings_results(outcome: outcome, empty: empty)
        </div>
    })
}

#[cfg(test)]
mod listings_page_tests {
    use super::*;
    use common::models::StayTotal;
    use rust_decimal::Decimal;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use topcoat::view::ViewExt;
    use uuid::Uuid;

    fn parse(url_query: &str) -> SearchQuery {
        match serde_urlencoded::from_str(url_query) {
            Ok(query) => query,
            Err(e) => panic!("{url_query}: {e}"),
        }
    }

    fn listing(stay_total: Option<StayTotal>) -> ListingResponse {
        let response: Result<ListingResponse, _> = serde_json::from_value(serde_json::json!({
            "id": Uuid::now_v7(),
            "user_id": Uuid::now_v7(),
            "name": "Sea Grape",
            "listing_structure": "Villa",
            "country": "Jamaica",
            "price_per_night": "650",
            "is_active": true,
            "added_at": "2030-01-01T00:00:00Z",
            "max_guests": 4,
            "bedrooms": 2,
            "beds": 2,
            "full_bathrooms": 2,
            "half_bathrooms": 0,
            "slug": "sea-grape",
            "base_currency": "USD",
            "minimum_stay": 1,
            "days_between_bookings": 0,
            "guests_included": 1,
            "stay_total": stay_total,
        }));
        match response {
            Ok(listing) => listing,
            Err(e) => panic!("{e}"),
        }
    }

    async fn html(v: impl View) -> String {
        let cx = Cx::default();
        match v.first().await {
            Ok(handle) => handle.render(&cx),
            Err(e) => panic!("render failed: {e}"),
        }
    }

    async fn results(outcome: SearchOutcome, empty: &str) -> String {
        let cx = &Cx::default();
        let empty = empty.to_string();
        html(view! { listings_results(outcome: outcome, empty: empty) }).await
    }

    #[derive(Clone, Default)]
    struct Logged(Arc<Mutex<Vec<u8>>>);

    impl Write for Logged {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Ok(mut lines) = self.0.lock() {
                lines.extend_from_slice(buf);
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Logged {
        type Writer = Self;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn a_bad_date_doesnt_lose_the_rest_of_the_search() {
        let query = parse("q=Runaway&check_in=abc&check_out=&guests=two&host=Sea-Grape");
        assert_eq!(query.q.as_deref(), Some("Runaway"));
        assert_eq!(query.check_in.as_deref(), Some("abc"));
        assert_eq!(query.guests.as_deref(), Some("two"));
        let params = query.params(None);
        assert_eq!(params.q.as_deref(), Some("Runaway"));
        assert_eq!(params.check_in.as_deref(), Some("abc"));
        assert_eq!(params.check_out, None);
        assert_eq!(params.guests.as_deref(), Some("two"));
        assert_eq!(params.host.as_deref(), Some("sea-grape"));
    }

    #[test]
    fn nothing_else_in_the_url_reaches_the_api() {
        let query = parse("include_inactive=true&owner=a%40b.com&user_id=abc&q=x");
        let params = query.params(None);
        assert!(!params.include_inactive);
        assert_eq!(params.owner_email, None);
        assert_eq!(params.user_id, None);
        assert_eq!(params.per_page, Some(SEARCH_PAGE_SIZE));
    }

    #[test]
    fn found_listings_carry_the_search() {
        let query = parse("q=x&check_in=2030-08-01&check_out=2030-08-04");
        let outcome = classify(Ok(vec![listing(None)]), &query, Some("JMD".to_string()));
        let SearchOutcome::Found(found) = outcome else {
            panic!("expected listings");
        };
        assert!(found[0].stay_searched);
        assert_eq!(
            found[0].search_query.as_deref(),
            Some("check_in=2030-08-01&check_out=2030-08-04&guests=1&currency=JMD")
        );
    }

    #[test]
    fn a_total_reaches_the_card() {
        let total = StayTotal {
            nights: 3,
            total_amount: Decimal::new(72_000, 2),
            damage_deposit: Decimal::ZERO,
            currency: "USD".to_string(),
        };
        let query = parse("check_in=2030-08-01&check_out=2030-08-04");
        let outcome = classify(Ok(vec![listing(Some(total.clone()))]), &query, None);
        let SearchOutcome::Found(found) = outcome else {
            panic!("expected listings");
        };
        assert_eq!(found[0].stay_total, Some(total));
    }

    #[test]
    fn a_refusal_keeps_its_message() {
        let refusal = ListingSearchRefusal {
            code: "INVALID_STAY_DATES".to_string(),
            message: "Invalid date range: dates can't be in the past.".to_string(),
        };
        assert_eq!(
            classify(Err(refusal.into()), &SearchQuery::default(), None),
            SearchOutcome::Refused("Invalid date range: dates can't be in the past.".to_string())
        );
    }

    #[test]
    fn any_other_failure_is_logged_without_the_search_text() {
        let logs = Logged::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(logs.clone())
            .finish();
        let error = anyhow::anyhow!("connection refused for /listings?q=SEARCHMARKER")
            .context("Failed to connect to listing service");
        let outcome = tracing::subscriber::with_default(subscriber, || {
            tracing::callsite::rebuild_interest_cache();
            classify(Err(error), &SearchQuery::default(), None)
        });

        assert_eq!(outcome, SearchOutcome::Failed);
        let text = match logs.0.lock() {
            Ok(lines) => String::from_utf8_lossy(&lines).into_owned(),
            Err(e) => panic!("{e}"),
        };
        assert!(text.contains("listing search failed"), "{text}");
        assert!(text.contains("Failed to connect"), "{text}");
        assert!(!text.contains("SEARCHMARKER"), "{text}");
    }

    #[tokio::test]
    async fn a_refusal_shows_its_message_and_no_listings() {
        let out = results(
            SearchOutcome::Refused("Invalid date range: dates can't be in the past.".to_string()),
            "No listings",
        )
        .await;
        assert!(out.contains(r#"role="alert""#), "{out}");
        assert!(out.contains("dates can"), "{out}");
        assert!(!out.contains("listings-grid"), "{out}");
        assert!(!out.contains("No listings"), "{out}");
    }

    #[tokio::test]
    async fn a_failure_never_reads_as_no_listings() {
        let out = results(SearchOutcome::Failed, "No listings").await;
        assert!(out.contains(SEARCH_FAILED), "{out}");
        assert!(out.contains(r#"role="alert""#), "{out}");
        assert!(!out.contains("No listings"), "{out}");
    }

    #[tokio::test]
    async fn nothing_found_shows_the_empty_state() {
        let out = results(SearchOutcome::Found(vec![]), "Nothing here").await;
        assert!(out.contains("Nothing here"), "{out}");
        assert!(!out.contains("role=\"alert\""), "{out}");
    }

    #[test]
    fn the_stays_line_shows_only_when_the_search_worked() {
        assert_eq!(
            stays_line(&SearchOutcome::Found(vec![])).as_deref(),
            Some("0 stays across Jamaica")
        );
        assert_eq!(stays_line(&SearchOutcome::Refused("x".to_string())), None);
        assert_eq!(stays_line(&SearchOutcome::Failed), None);
    }

    #[test]
    fn the_empty_state_mentions_dates_when_they_were_given() {
        assert!(
            empty_message(&parse("check_in=2030-08-01&check_out=2030-08-04")).contains("dates")
        );
        assert_eq!(
            empty_message(&parse("host=sea-grape")),
            "This host has no listings yet"
        );
        assert_eq!(
            empty_message(&parse("q=x")),
            "No listings match your search criteria"
        );
    }

    #[test]
    fn show_all_listings_removes_only_the_host() {
        let query = parse(
            "host=sea-grape&q=Runaway%20Bay&check_in=2030-08-01&check_out=2030-08-04&guests=2",
        );
        assert_eq!(
            query.all_listings_href(Some("JMD".to_string())),
            "/listings?q=Runaway%20Bay&check_in=2030-08-01&check_out=2030-08-04&guests=2&currency=JMD"
        );
        assert_eq!(parse("host=sea-grape").all_listings_href(None), "/listings");
    }

    #[tokio::test]
    async fn the_form_is_filled_in_from_the_url() {
        let cx = &Cx::default();
        let query =
            parse("q=Runaway&check_in=2030-08-01&check_out=2030-08-04&guests=2&host=Sea-Grape");
        let out = html(view! {
            listing_search_form(query: query, today: "2030-06-15".to_string(), currency: Some("JMD".to_string()))
        })
        .await;
        for expected in [
            r#"value="Runaway""#,
            r#"value="2030-08-01""#,
            r#"value="2030-08-04""#,
            r#"value="2""#,
            r#"name="host" value="sea-grape""#,
            r#"name="currency" value="JMD""#,
            r#"min="2030-06-15""#,
            "Search by name or place",
        ] {
            assert!(out.contains(expected), "{expected}: {out}");
        }
    }
}
