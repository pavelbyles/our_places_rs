//! The "Staff & services" fieldset of the listing studio form (spec-122
//! §2.5): included staff one per line, a note, and fixed add-on rows of
//! name, price and unit. An add-on row without a name is ignored.

use std::collections::BTreeMap;
use std::str::FromStr;

use common::listing_details::{
    MAX_ADDON_PRICE, MAX_ADDON_UNIT_CHARS, MAX_ADDONS, MAX_INCLUDED_SERVICES, MAX_SERVICE_CHARS,
    MAX_SERVICE_NOTE_CHARS, STAFF_SERVICES_KEY, ServiceAddon, StaffServices,
};
use rust_decimal::Decimal;
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};

/// Add-on rows the form offers.
pub const ADDON_SLOTS: usize = MAX_ADDONS;
/// Form field for included staff, one per line.
pub const INCLUDED_FIELD: &str = "services_included";
/// Form field for the note under the included list.
pub const NOTE_FIELD: &str = "services_note";

/// Form field name for add-on `n` (from 1): `addon_2_price`.
pub fn addon_key(n: usize, part: &str) -> String {
    format!("addon_{n}_{part}")
}

/// The form values for a listing's stored staff and services.
pub fn services_fields(details: Option<&serde_json::Value>) -> BTreeMap<String, String> {
    let s: StaffServices = details
        .and_then(|d| d.get(STAFF_SERVICES_KEY))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let mut fields = BTreeMap::from([
        (INCLUDED_FIELD.to_string(), s.included.join("\n")),
        (NOTE_FIELD.to_string(), s.included_note.unwrap_or_default()),
    ]);
    for (i, a) in s.addons.into_iter().take(ADDON_SLOTS).enumerate() {
        let n = i + 1;
        fields.insert(addon_key(n, "name"), a.name);
        fields.insert(
            addon_key(n, "price"),
            a.price
                .map(|p| p.normalize().to_string())
                .unwrap_or_default(),
        );
        fields.insert(addon_key(n, "unit"), a.unit.unwrap_or_default());
    }
    fields
}

/// Staff and services as posted, or the reasons they can't be saved.
/// `Ok(None)` when nothing is filled in.
pub fn parse_services(
    fields: &BTreeMap<String, String>,
) -> std::result::Result<Option<StaffServices>, Vec<String>> {
    let get = |key: &str| {
        fields
            .get(key)
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let chars = |s: &str| s.chars().count();
    let mut errors = Vec::new();
    let included: Vec<String> = get(INCLUDED_FIELD)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if included.len() > MAX_INCLUDED_SERVICES
        || included.iter().any(|i| chars(i) > MAX_SERVICE_CHARS)
    {
        errors.push(format!(
            "List at most {MAX_INCLUDED_SERVICES} included services of up to {MAX_SERVICE_CHARS} characters each."
        ));
    }
    let note = get(NOTE_FIELD);
    if chars(&note) > MAX_SERVICE_NOTE_CHARS {
        errors.push(format!(
            "The note under included services can be at most {MAX_SERVICE_NOTE_CHARS} characters."
        ));
    }
    let mut addons = Vec::new();
    for n in 1..=ADDON_SLOTS {
        let (name, price, unit) = (
            get(&addon_key(n, "name")),
            get(&addon_key(n, "price")),
            get(&addon_key(n, "unit")),
        );
        if name.is_empty() {
            if !price.is_empty() || !unit.is_empty() {
                errors.push(format!("Add-on {n} needs a name."));
            }
            continue;
        }
        if chars(&name) > MAX_SERVICE_CHARS || chars(&unit) > MAX_ADDON_UNIT_CHARS {
            errors.push(format!(
                "Add-on {n}'s name can be at most {MAX_SERVICE_CHARS} characters and its unit {MAX_ADDON_UNIT_CHARS}."
            ));
        }
        let price = if price.is_empty() {
            None
        } else {
            Decimal::from_str(&price.replace(',', ""))
                .ok()
                .filter(|p| {
                    *p >= Decimal::ZERO && *p <= Decimal::from(MAX_ADDON_PRICE) && p.scale() <= 2
                })
                .or_else(|| {
                    errors.push(format!(
                        "Add-on {n}'s price must be a number from 0 to 1,000,000 with at most 2 decimals, or empty for \u{201c}On request\u{201d}."
                    ));
                    None
                })
        };
        addons.push(ServiceAddon {
            name,
            price,
            unit: Some(unit).filter(|u| !u.is_empty()),
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let services = StaffServices {
        included,
        included_note: Some(note).filter(|n| !n.is_empty()),
        addons,
    };
    Ok((!services.is_empty()).then_some(services))
}

/// The fieldset.
#[component]
pub async fn services_fieldset(
    cx: &Cx,
    /// Current form values.
    fields: BTreeMap<String, String>,
    /// Class for field labels.
    label_class: &'static str,
    /// Class for inputs.
    input_class: &'static str,
) -> Result<impl View> {
    let get = |key: &str| fields.get(key).cloned().unwrap_or_default();
    let included = get(INCLUDED_FIELD);
    let note = get(NOTE_FIELD);
    let rows: Vec<(usize, String, String, String)> = (1..=ADDON_SLOTS)
        .map(|n| {
            (
                n,
                get(&addon_key(n, "name")),
                get(&addon_key(n, "price")),
                get(&addon_key(n, "unit")),
            )
        })
        .collect();
    Ok(view! {
        <fieldset id="services-editor" class="grid gap-4 rounded-xl border border-border p-6">
            <legend class="px-1 text-sm font-semibold">"Staff & services"</legend>
            <div class="grid gap-4 md:grid-cols-2">
                <div class="grid content-start gap-2">
                    <label for="listing-services-included" class=(label_class)>
                        "Included with the stay, one per line (e.g. Private chef, Daily housekeeping)"
                    </label>
                    <textarea
                        id="listing-services-included"
                        name=(INCLUDED_FIELD)
                        rows="5"
                        class=(format!("{input_class} h-auto py-2"))
                    >
                        (included)
                    </textarea>
                    <label class=(label_class)>
                        "Note under the list (optional)"
                        <input
                            name=(NOTE_FIELD)
                            maxlength=(MAX_SERVICE_NOTE_CHARS.to_string())
                            placeholder="Groceries are billed at cost."
                            value=(note)
                            class=(input_class)
                        />
                    </label>
                </div>
                <div class="grid content-start gap-2">
                    <p class=(label_class)>
                        "Paid add-ons. Prices are in the listing currency; leave the price empty for \u{201c}On request\u{201d}."
                    </p>
                    for (n, name, price, unit) in rows {
                        <div data-addon-row=(n.to_string()) class="grid grid-cols-[1fr_6rem_6rem] gap-2">
                            <input
                                name=(addon_key(n, "name"))
                                aria-label=(format!("Add-on {n} name"))
                                placeholder=(if n == 1 { "Airport transfer" } else { "" })
                                value=(name)
                                class=(input_class)
                            />
                            <input
                                name=(addon_key(n, "price"))
                                aria-label=(format!("Add-on {n} price"))
                                inputmode="decimal"
                                placeholder=(if n == 1 { "120" } else { "" })
                                value=(price)
                                class=(input_class)
                            />
                            <input
                                name=(addon_key(n, "unit"))
                                aria-label=(format!("Add-on {n} unit"))
                                maxlength=(MAX_ADDON_UNIT_CHARS.to_string())
                                placeholder=(if n == 1 { "each way" } else { "" })
                                value=(unit)
                                class=(input_class)
                            />
                        </div>
                    }
                </div>
            </div>
        </fieldset>
    })
}
