//! The "Suites" fieldset of the listing studio form (spec-122 §2.3): fixed
//! slots of name, beds, guests, bathroom, features and a photo picked from
//! the listing's own images. A slot with no name and no beds is ignored.

use std::collections::BTreeMap;

use common::listing_details::{
    MAX_SUITE_BATH_CHARS, MAX_SUITE_BEDS_CHARS, MAX_SUITE_FEATURE_CHARS, MAX_SUITE_FEATURES,
    MAX_SUITE_GUESTS, MAX_SUITE_NAME_CHARS, MAX_SUITES, SUITES_KEY, SuiteItem,
};
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};
use uuid::Uuid;

/// Suite slots the form offers.
pub const SUITE_SLOTS: usize = MAX_SUITES;

/// A photo the host can pick for a suite: (image id, label).
pub type SuitePhoto = (String, String);

/// Form field name for suite `n` (from 1): `suite_2_beds`.
pub fn suite_key(n: usize, part: &str) -> String {
    format!("suite_{n}_{part}")
}

/// The form values for a listing's stored suites.
pub fn suite_fields(details: Option<&serde_json::Value>) -> BTreeMap<String, String> {
    details
        .and_then(|d| d.get(SUITES_KEY))
        .and_then(|s| serde_json::from_value::<Vec<SuiteItem>>(s.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .take(SUITE_SLOTS)
        .enumerate()
        .flat_map(|(i, s)| {
            let n = i + 1;
            [
                (suite_key(n, "name"), s.name),
                (suite_key(n, "beds"), s.bed_configuration),
                (suite_key(n, "guests"), s.max_guests.to_string()),
                (suite_key(n, "bath"), s.en_suite.unwrap_or_default()),
                (suite_key(n, "features"), s.features.join(", ")),
                (
                    suite_key(n, "image"),
                    s.image_id.map(|id| id.to_string()).unwrap_or_default(),
                ),
            ]
        })
        .collect()
}

/// The suites as posted, in slot order, or the reasons they can't be saved
/// (naming the suite). Features are comma-separated.
pub fn parse_suites(
    fields: &BTreeMap<String, String>,
) -> std::result::Result<Vec<SuiteItem>, Vec<String>> {
    let get = |n: usize, part: &str| {
        fields
            .get(&suite_key(n, part))
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let chars = |s: &str| s.chars().count();
    let mut errors = Vec::new();
    let mut suites = Vec::new();
    for n in 1..=SUITE_SLOTS {
        let (name, beds) = (get(n, "name"), get(n, "beds"));
        if name.is_empty() && beds.is_empty() {
            continue;
        }
        if name.is_empty() || beds.is_empty() {
            errors.push(format!("Suite {n} needs a name and a bed setup."));
            continue;
        }
        if chars(&name) > MAX_SUITE_NAME_CHARS || chars(&beds) > MAX_SUITE_BEDS_CHARS {
            errors.push(format!(
                "Suite {n}'s name can be at most {MAX_SUITE_NAME_CHARS} characters and its bed setup {MAX_SUITE_BEDS_CHARS}."
            ));
        }
        let guests = get(n, "guests")
            .parse::<i32>()
            .ok()
            .filter(|g| (1..=MAX_SUITE_GUESTS).contains(g))
            .unwrap_or_else(|| {
                errors.push(format!(
                    "Suite {n}'s guests must be a whole number from 1 to {MAX_SUITE_GUESTS}."
                ));
                0
            });
        let features: Vec<String> = get(n, "features")
            .split(',')
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(str::to_string)
            .collect();
        if features.len() > MAX_SUITE_FEATURES
            || features.iter().any(|f| chars(f) > MAX_SUITE_FEATURE_CHARS)
        {
            errors.push(format!(
                "Suite {n} can have at most {MAX_SUITE_FEATURES} features of up to {MAX_SUITE_FEATURE_CHARS} characters."
            ));
        }
        let bath = get(n, "bath");
        if chars(&bath) > MAX_SUITE_BATH_CHARS {
            errors.push(format!(
                "Suite {n}'s bathroom can be at most {MAX_SUITE_BATH_CHARS} characters."
            ));
        }
        let image = get(n, "image");
        let image_id = if image.is_empty() {
            None
        } else {
            Uuid::parse_str(&image).ok().or_else(|| {
                errors.push(format!("Choose a photo from the list for suite {n}."));
                None
            })
        };
        suites.push(SuiteItem {
            name,
            bed_configuration: beds,
            max_guests: guests,
            features,
            en_suite: Some(bath).filter(|b| !b.is_empty()),
            image_id,
        });
    }
    if errors.is_empty() {
        Ok(suites)
    } else {
        Err(errors)
    }
}

/// The fieldset: suite slots in a two-column grid.
#[component]
pub async fn suite_fieldset(
    cx: &Cx,
    /// Current form values (`suite_{n}_{part}`).
    fields: BTreeMap<String, String>,
    /// The listing's photos to choose from (empty for a new listing).
    photos: Vec<SuitePhoto>,
    /// Class for field labels.
    label_class: &'static str,
    /// Class for inputs and selects.
    input_class: &'static str,
) -> Result<impl View> {
    let slots: Vec<(usize, BTreeMap<&'static str, String>)> = (1..=SUITE_SLOTS)
        .map(|n| {
            let values = ["name", "beds", "guests", "bath", "features", "image"]
                .into_iter()
                .map(|part| {
                    (
                        part,
                        fields.get(&suite_key(n, part)).cloned().unwrap_or_default(),
                    )
                })
                .collect();
            (n, values)
        })
        .collect();
    let no_photos = photos.is_empty();
    Ok(view! {
        <fieldset id="suites-editor" class="grid gap-4 rounded-xl border border-border p-6">
            <legend class="px-1 text-sm font-semibold">"Suites"</legend>
            <p class="text-xs text-muted-foreground">
                "Describe each bedroom guests can book into, up to 8. Leave a suite's name and beds empty to skip it. Features are comma-separated, e.g. Ocean view, Private balcony."
                if no_photos {
                    " Save the listing and upload photos to pick a photo for each suite."
                }
            </p>
            <div class="grid gap-4 md:grid-cols-2">
                for (n, v) in slots {
                    <div
                        data-suite-slot=(n.to_string())
                        class="grid content-start gap-2 rounded-lg border border-border p-4"
                    >
                        <div class="grid gap-2 sm:grid-cols-[1fr_8rem]">
                            <label class=(label_class)>
                                (format!("Suite {n} name"))
                                <input
                                    id=(format!("listing-suite-{n}-name"))
                                    name=(suite_key(n, "name"))
                                    maxlength=(MAX_SUITE_NAME_CHARS.to_string())
                                    value=(v.get("name").cloned().unwrap_or_default())
                                    class=(input_class)
                                />
                            </label>
                            <label class=(label_class)>
                                "Guests"
                                <input
                                    name=(suite_key(n, "guests"))
                                    type="number"
                                    min="1"
                                    max=(MAX_SUITE_GUESTS.to_string())
                                    value=(v.get("guests").cloned().unwrap_or_default())
                                    class=(input_class)
                                />
                            </label>
                        </div>
                        <div class="grid gap-2 sm:grid-cols-2">
                            <label class=(label_class)>
                                "Beds"
                                <input
                                    name=(suite_key(n, "beds"))
                                    maxlength=(MAX_SUITE_BEDS_CHARS.to_string())
                                    placeholder="1 king"
                                    value=(v.get("beds").cloned().unwrap_or_default())
                                    class=(input_class)
                                />
                            </label>
                            <label class=(label_class)>
                                "Bathroom"
                                <input
                                    name=(suite_key(n, "bath"))
                                    maxlength=(MAX_SUITE_BATH_CHARS.to_string())
                                    placeholder="En-suite, soaking tub"
                                    value=(v.get("bath").cloned().unwrap_or_default())
                                    class=(input_class)
                                />
                            </label>
                        </div>
                        <label class=(label_class)>
                            "Features"
                            <input
                                name=(suite_key(n, "features"))
                                placeholder="Ocean view, Private balcony"
                                value=(v.get("features").cloned().unwrap_or_default())
                                class=(input_class)
                            />
                        </label>
                        <label class=(label_class)>
                            "Photo"
                            <select
                                name=(suite_key(n, "image"))
                                disabled=(no_photos)
                                class=(input_class)
                            >
                                <option value="">"No photo"</option>
                                for (id, label) in photos.clone() {
                                    <option
                                        value=(id.clone())
                                        selected=(v.get("image").is_some_and(|i| *i == id))
                                    >
                                        (label)
                                    </option>
                                }
                            </select>
                        </label>
                    </div>
                }
            </div>
        </fieldset>
    })
}
