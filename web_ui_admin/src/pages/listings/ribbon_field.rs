//! The "Highlights ribbon" fieldset of the listing studio form (spec-122
//! §2.2): six fixed rows of icon, value and label. Empty rows are ignored,
//! so it works without JavaScript; `ui.js` adds reordering and a preview.

use std::collections::BTreeMap;

use common::listing_details::ribbon::{
    MAX_RIBBON_ITEMS, MAX_RIBBON_LABEL_CHARS, MAX_RIBBON_VALUE_CHARS,
};
use common::listing_details::{RIBBON_KEY, Ribbon, RibbonIcon, RibbonItem};
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};

/// Rows the form offers.
pub const RIBBON_SLOTS: usize = MAX_RIBBON_ITEMS;

/// Form field name for row `n` (from 1): `ribbon_3_value`.
pub fn ribbon_key(n: usize, part: &str) -> String {
    format!("ribbon_{n}_{part}")
}

/// The form values for a listing's stored ribbon (rows from 1).
pub fn ribbon_fields(details: Option<&serde_json::Value>) -> BTreeMap<String, String> {
    details
        .and_then(|d| d.get(RIBBON_KEY))
        .and_then(|r| serde_json::from_value::<Vec<RibbonItem>>(r.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .take(RIBBON_SLOTS)
        .enumerate()
        .flat_map(|(i, item)| {
            let n = i + 1;
            [
                (ribbon_key(n, "icon"), item.icon.key().to_string()),
                (ribbon_key(n, "value"), item.value),
                (ribbon_key(n, "label"), item.label),
            ]
        })
        .collect()
}

/// The ribbon as posted: `Ok(None)` when every row is empty (the guest page
/// then shows the listing facts), or the reasons it can't be saved, naming
/// the row.
pub fn parse_ribbon(
    fields: &BTreeMap<String, String>,
) -> std::result::Result<Option<Vec<RibbonItem>>, Vec<String>> {
    let get = |n: usize, part: &str| {
        fields
            .get(&ribbon_key(n, part))
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let mut errors = Vec::new();
    let mut items = Vec::new();
    for n in 1..=RIBBON_SLOTS {
        let (value, label) = (get(n, "value"), get(n, "label"));
        if value.is_empty() && label.is_empty() {
            continue;
        }
        if value.is_empty() || label.is_empty() {
            errors.push(format!("Ribbon item {n} needs a value and a label."));
            continue;
        }
        if value.chars().count() > MAX_RIBBON_VALUE_CHARS {
            errors.push(format!(
                "Ribbon item {n}'s value can be at most {MAX_RIBBON_VALUE_CHARS} characters."
            ));
        }
        if label.chars().count() > MAX_RIBBON_LABEL_CHARS {
            errors.push(format!(
                "Ribbon item {n}'s label can be at most {MAX_RIBBON_LABEL_CHARS} characters."
            ));
        }
        match RibbonIcon::from_key(&get(n, "icon")) {
            Some(icon) => items.push(RibbonItem { icon, value, label }),
            None => errors.push(format!("Choose an icon for ribbon item {n}.")),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if items.is_empty() {
        return Ok(None);
    }
    let filled = items.len();
    Ribbon::try_from(items.clone()).map(|_| Some(items)).map_err(|_| {
        vec![format!(
            "Fill in 3 to 6 ribbon items, or leave them all empty to show bedrooms, bathrooms and guests (you filled in {filled})."
        )]
    })
}

/// The fieldset: six rows of icon, value and label.
#[component]
pub async fn ribbon_fieldset(
    cx: &Cx,
    /// Current form values (`ribbon_{n}_{icon|value|label}`).
    fields: BTreeMap<String, String>,
    /// Class for field labels.
    label_class: &'static str,
    /// Class for inputs and selects.
    input_class: &'static str,
) -> Result<impl View> {
    let rows: Vec<(usize, String, String, String)> = (1..=RIBBON_SLOTS)
        .map(|n| {
            let get = |part: &str| {
                fields
                    .get(&ribbon_key(n, part))
                    .cloned()
                    .unwrap_or_default()
            };
            (n, get("icon"), get("value"), get("label"))
        })
        .collect();
    Ok(view! {
        <fieldset
            id="ribbon-editor"
            data-ribbon-editor=""
            class="grid gap-3 rounded-xl border border-border p-6"
        >
            <legend class="px-1 text-sm font-semibold">"Highlights ribbon"</legend>
            <p class="text-xs text-muted-foreground">
                "The band of 3 to 6 key items guests see first on your listing, e.g. “6 · Bedrooms” or “Direct · Beach access”. Keep values short. Leave every row empty to show bedrooms, bathrooms and maximum guests."
            </p>
            <div
                aria-hidden="true"
                class="hidden grid-cols-[10rem_7rem_1fr] gap-2 text-xs font-medium text-muted-foreground sm:grid"
            >
                <span>"Icon"</span>
                <span>"Value"</span>
                <span>"Label"</span>
            </div>
            for (n, icon, value, label) in rows {
                <div
                    data-ribbon-row=(n.to_string())
                    class="grid gap-2 sm:grid-cols-[10rem_7rem_1fr]"
                >
                    <label class=(label_class)>
                        <span class="sm:sr-only">(format!("Item {n} icon"))</span>
                        <select
                            name=(ribbon_key(n, "icon"))
                            aria-label=(format!("Ribbon item {n} icon"))
                            class=(input_class)
                        >
                            for choice in RibbonIcon::ALL {
                                <option
                                    value=(choice.key())
                                    selected=(icon == choice.key())
                                >
                                    (choice.label())
                                </option>
                            }
                        </select>
                    </label>
                    <label class=(label_class)>
                        <span class="sm:sr-only">(format!("Item {n} value"))</span>
                        <input
                            name=(ribbon_key(n, "value"))
                            aria-label=(format!("Ribbon item {n} value"))
                            maxlength=(MAX_RIBBON_VALUE_CHARS.to_string())
                            placeholder=(if n == 1 { "6" } else { "" })
                            value=(value)
                            class=(input_class)
                        />
                    </label>
                    <label class=(label_class)>
                        <span class="sm:sr-only">(format!("Item {n} label"))</span>
                        <input
                            name=(ribbon_key(n, "label"))
                            aria-label=(format!("Ribbon item {n} label"))
                            maxlength=(MAX_RIBBON_LABEL_CHARS.to_string())
                            placeholder=(if n == 1 { "Bedrooms" } else { "" })
                            value=(label)
                            class=(input_class)
                        />
                    </label>
                </div>
            }
        </fieldset>
    })
}
