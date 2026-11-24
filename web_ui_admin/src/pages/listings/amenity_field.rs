//! The "Amenities" fieldset of the listing studio form (spec-122 §2.4): six
//! fixed group slots of title, icon and items (one per line). A slot with no
//! items is ignored, so the suggested titles a new listing starts with need
//! no clean-up.

use std::collections::BTreeMap;

use common::listing_details::amenities::{
    MAX_AMENITIES, MAX_AMENITY_CHARS, MAX_AMENITY_GROUPS, MAX_GROUP_ITEMS, MAX_GROUP_TITLE_CHARS,
};
use common::listing_details::{
    AMENITIES_KEY, AMENITY_GROUPS_KEY, AmenityGroup, AmenityIcon, SUGGESTED_AMENITY_GROUPS,
    amenity_groups_for,
};
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};

/// Group slots the form offers.
pub const AMENITY_SLOTS: usize = MAX_AMENITY_GROUPS;

/// Form field name for group `n` (from 1): `group_2_items`.
pub fn group_key(n: usize, part: &str) -> String {
    format!("group_{n}_{part}")
}

fn slots_of(groups: &[AmenityGroup]) -> BTreeMap<String, String> {
    groups
        .iter()
        .take(AMENITY_SLOTS)
        .enumerate()
        .flat_map(|(i, g)| {
            let n = i + 1;
            [
                (group_key(n, "title"), g.title.clone()),
                (group_key(n, "icon"), g.icon.key().to_string()),
                (group_key(n, "items"), g.items.join("\n")),
            ]
        })
        .collect()
}

/// The groups stored in `listing_details`: the host's groups, else a legacy
/// flat list as one "Amenities" group.
pub fn amenity_groups_of(details: Option<&serde_json::Value>) -> Vec<AmenityGroup> {
    let groups = details
        .and_then(|d| d.get(AMENITY_GROUPS_KEY))
        .and_then(|g| serde_json::from_value::<Vec<AmenityGroup>>(g.clone()).ok());
    let legacy: Vec<String> = details
        .and_then(|d| d.get(AMENITIES_KEY))
        .and_then(|a| serde_json::from_value(a.clone()).ok())
        .unwrap_or_default();
    amenity_groups_for(groups.as_deref(), &legacy)
}

/// The form values for a listing's amenities.
pub fn amenity_fields(details: Option<&serde_json::Value>) -> BTreeMap<String, String> {
    slots_of(&amenity_groups_of(details))
}

/// The suggested group titles a new listing starts with.
pub fn suggested_amenity_fields() -> BTreeMap<String, String> {
    let groups: Vec<AmenityGroup> = SUGGESTED_AMENITY_GROUPS
        .iter()
        .map(|(title, icon)| AmenityGroup {
            title: (*title).to_string(),
            icon: *icon,
            items: Vec::new(),
        })
        .collect();
    slots_of(&groups)
}

/// The groups as posted, in slot order, or the reasons they can't be saved
/// (naming the group). Lines are trimmed, blanks and case-insensitive
/// duplicates within a group dropped.
pub fn parse_amenity_groups(
    fields: &BTreeMap<String, String>,
) -> std::result::Result<Vec<AmenityGroup>, Vec<String>> {
    let get = |n: usize, part: &str| fields.get(&group_key(n, part)).cloned().unwrap_or_default();
    let mut errors = Vec::new();
    let mut groups = Vec::new();
    for n in 1..=AMENITY_SLOTS {
        let mut seen = std::collections::HashSet::new();
        let items: Vec<String> = get(n, "items")
            .lines()
            .map(str::trim)
            .filter(|a| !a.is_empty() && seen.insert(a.to_lowercase()))
            .map(str::to_string)
            .collect();
        if items.is_empty() {
            continue;
        }
        let title = get(n, "title").trim().to_string();
        if title.is_empty() {
            errors.push(format!("Amenity group {n} needs a title."));
        } else if title.chars().count() > MAX_GROUP_TITLE_CHARS {
            errors.push(format!(
                "Amenity group {n}'s title can be at most {MAX_GROUP_TITLE_CHARS} characters."
            ));
        }
        if items.len() > MAX_GROUP_ITEMS {
            errors.push(format!(
                "Amenity group {n} can list at most {MAX_GROUP_ITEMS} items."
            ));
        }
        if let Some(long) = items.iter().find(|a| a.chars().count() > MAX_AMENITY_CHARS) {
            errors.push(format!(
                "Amenities can be at most {MAX_AMENITY_CHARS} characters (\"{long}\" is longer)."
            ));
        }
        let icon = AmenityIcon::from_key(get(n, "icon").trim()).unwrap_or(AmenityIcon::Home);
        groups.push(AmenityGroup { title, icon, items });
    }
    let total: usize = groups.iter().map(|g| g.items.len()).sum();
    if total > MAX_AMENITIES {
        errors.push(format!("List at most {MAX_AMENITIES} amenities."));
    }
    if errors.is_empty() {
        Ok(groups)
    } else {
        Err(errors)
    }
}

/// The fieldset: six group slots.
#[component]
pub async fn amenity_fieldset(
    cx: &Cx,
    /// Current form values (`group_{n}_{title|icon|items}`).
    fields: BTreeMap<String, String>,
    /// Class for field labels.
    label_class: &'static str,
    /// Class for inputs and selects.
    input_class: &'static str,
) -> Result<impl View> {
    let slots: Vec<(usize, String, String, String)> = (1..=AMENITY_SLOTS)
        .map(|n| {
            let get = |part: &str| fields.get(&group_key(n, part)).cloned().unwrap_or_default();
            (n, get("title"), get("icon"), get("items"))
        })
        .collect();
    Ok(view! {
        <fieldset
            id="amenity-groups-editor"
            data-amenity-editor=""
            class="grid gap-4 rounded-xl border border-border p-6"
        >
            <legend class="px-1 text-sm font-semibold">"Amenities"</legend>
            <p class="text-xs text-muted-foreground">
                "Group what the house offers the way guests think about it, up to 6 groups and 50 amenities in total. One amenity per line. A group with no amenities is left out."
            </p>
            <div class="grid gap-4 md:grid-cols-2">
                for (n, title, icon, items) in slots {
                    <div
                        data-amenity-slot=(n.to_string())
                        class="grid content-start gap-2 rounded-lg border border-border p-4"
                    >
                        <div class="grid gap-2 sm:grid-cols-[1fr_9rem]">
                            <label class=(label_class)>
                                (format!("Group {n} title"))
                                <input
                                    id=(format!("listing-amenity-group-{n}-title"))
                                    name=(group_key(n, "title"))
                                    maxlength=(MAX_GROUP_TITLE_CHARS.to_string())
                                    value=(title)
                                    class=(input_class)
                                />
                            </label>
                            <label class=(label_class)>
                                "Icon"
                                <select name=(group_key(n, "icon")) class=(input_class)>
                                    for choice in AmenityIcon::ALL {
                                        <option
                                            value=(choice.key())
                                            selected=(icon == choice.key())
                                        >
                                            (choice.label())
                                        </option>
                                    }
                                </select>
                            </label>
                        </div>
                        <label class=(label_class)>
                            "Amenities, one per line"
                            <textarea
                                id=(format!("listing-amenities-{n}"))
                                name=(group_key(n, "items"))
                                rows="4"
                                class=(format!("{input_class} h-auto py-2"))
                            >
                                (items)
                            </textarea>
                        </label>
                    </div>
                }
            </div>
        </fieldset>
    })
}
