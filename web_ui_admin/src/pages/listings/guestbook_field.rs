//! The "Guestbook quotes" fieldset of the listing studio form (spec-122
//! §2.7): up to four quotes guests left in the villa's own guestbook, shown
//! on the listing page only until verified reviews arrive, and always
//! labelled as guestbook quotes. A row without a quote is ignored.

use std::collections::BTreeMap;

use common::listing_details::{
    CuratedTestimonial, MAX_CREDIT_CHARS, MAX_QUOTE_CHARS, MAX_TESTIMONIALS, TESTIMONIALS_KEY,
};
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};

/// Quote rows the form offers.
pub const QUOTE_SLOTS: usize = MAX_TESTIMONIALS;

/// Form field name for quote `n` (from 1): `quote_2_text`.
pub fn quote_key(n: usize, part: &str) -> String {
    format!("quote_{n}_{part}")
}

/// The form values for a listing's stored guestbook quotes.
pub fn guestbook_fields(details: Option<&serde_json::Value>) -> BTreeMap<String, String> {
    details
        .and_then(|d| d.get(TESTIMONIALS_KEY))
        .and_then(|v| serde_json::from_value::<Vec<CuratedTestimonial>>(v.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .take(QUOTE_SLOTS)
        .enumerate()
        .flat_map(|(i, q)| {
            let n = i + 1;
            [
                (quote_key(n, "name"), q.guest_name),
                (quote_key(n, "location"), q.location.unwrap_or_default()),
                (quote_key(n, "date"), q.stay_date),
                (quote_key(n, "rating"), q.rating.to_string()),
                (quote_key(n, "text"), q.quote),
            ]
        })
        .collect()
}

/// The quotes as posted, or the reasons they can't be saved (naming the
/// quote).
pub fn parse_guestbook(
    fields: &BTreeMap<String, String>,
) -> std::result::Result<Vec<CuratedTestimonial>, Vec<String>> {
    let get = |n: usize, part: &str| {
        fields
            .get(&quote_key(n, part))
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let len = |s: &str| s.chars().count();
    let mut errors = Vec::new();
    let mut quotes = Vec::new();
    for n in 1..=QUOTE_SLOTS {
        let text = get(n, "text");
        let (name, location, date) = (get(n, "name"), get(n, "location"), get(n, "date"));
        if text.is_empty() {
            if !name.is_empty() || !date.is_empty() {
                errors.push(format!("Guestbook quote {n} needs the quote itself."));
            }
            continue;
        }
        if name.is_empty() || date.is_empty() {
            errors.push(format!(
                "Guestbook quote {n} needs the guest's first name and when they stayed."
            ));
        }
        if len(&text) > MAX_QUOTE_CHARS
            || [&name, &location, &date]
                .iter()
                .any(|s| len(s) > MAX_CREDIT_CHARS)
        {
            errors.push(format!(
                "Guestbook quote {n} can be at most {MAX_QUOTE_CHARS} characters, and its name, city and date {MAX_CREDIT_CHARS} each."
            ));
        }
        let rating = get(n, "rating")
            .parse::<i32>()
            .ok()
            .filter(|r| (1..=5).contains(r))
            .unwrap_or_else(|| {
                errors.push(format!("Guestbook quote {n}'s rating must be 1 to 5."));
                5
            });
        quotes.push(CuratedTestimonial {
            guest_name: name,
            location: Some(location).filter(|l| !l.is_empty()),
            stay_date: date,
            quote: text,
            rating,
        });
    }
    if errors.is_empty() {
        Ok(quotes)
    } else {
        Err(errors)
    }
}

/// The fieldset.
#[component]
pub async fn guestbook_fieldset(
    cx: &Cx,
    /// Current form values.
    fields: BTreeMap<String, String>,
    /// Class for field labels.
    label_class: &'static str,
    /// Class for inputs.
    input_class: &'static str,
) -> Result<impl View> {
    let rows: Vec<(usize, BTreeMap<&'static str, String>)> = (1..=QUOTE_SLOTS)
        .map(|n| {
            let values = ["name", "location", "date", "rating", "text"]
                .into_iter()
                .map(|part| {
                    (
                        part,
                        fields.get(&quote_key(n, part)).cloned().unwrap_or_default(),
                    )
                })
                .collect();
            (n, values)
        })
        .collect();
    Ok(view! {
        <fieldset id="guestbook-editor" class="grid gap-4 rounded-xl border border-border p-6">
            <legend class="px-1 text-sm font-semibold">"Guestbook quotes"</legend>
            <p class="text-xs text-muted-foreground">
                "Real quotes from your villa\u{2019}s own guestbook, up to 4. Guests see them, labelled as guestbook quotes, only until verified reviews from bookings arrive."
            </p>
            <div class="grid gap-4 md:grid-cols-2">
                for (n, v) in rows {
                    <div
                        data-quote-slot=(n.to_string())
                        class="grid content-start gap-2 rounded-lg border border-border p-4"
                    >
                        <label class=(label_class)>
                            (format!("Quote {n}"))
                            <textarea
                                name=(quote_key(n, "text"))
                                rows="3"
                                maxlength=(MAX_QUOTE_CHARS.to_string())
                                class=(format!("{input_class} h-auto py-2"))
                            >
                                (v.get("text").cloned().unwrap_or_default())
                            </textarea>
                        </label>
                        <div class="grid gap-2 sm:grid-cols-[1fr_1fr_5rem]">
                            <label class=(label_class)>
                                "Guest first name"
                                <input
                                    name=(quote_key(n, "name"))
                                    maxlength=(MAX_CREDIT_CHARS.to_string())
                                    value=(v.get("name").cloned().unwrap_or_default())
                                    class=(input_class)
                                />
                            </label>
                            <label class=(label_class)>
                                "Home city"
                                <input
                                    name=(quote_key(n, "location"))
                                    maxlength=(MAX_CREDIT_CHARS.to_string())
                                    value=(v.get("location").cloned().unwrap_or_default())
                                    class=(input_class)
                                />
                            </label>
                            <label class=(label_class)>
                                "Stars"
                                <input
                                    name=(quote_key(n, "rating"))
                                    type="number"
                                    min="1"
                                    max="5"
                                    value=(v.get("rating").cloned().unwrap_or_default())
                                    class=(input_class)
                                />
                            </label>
                        </div>
                        <label class=(label_class)>
                            "When they stayed"
                            <input
                                name=(quote_key(n, "date"))
                                maxlength=(MAX_CREDIT_CHARS.to_string())
                                placeholder="March 2026"
                                value=(v.get("date").cloned().unwrap_or_default())
                                class=(input_class)
                            />
                        </label>
                    </div>
                }
            </div>
        </fieldset>
    })
}
