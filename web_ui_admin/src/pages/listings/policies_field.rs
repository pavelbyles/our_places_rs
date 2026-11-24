//! The "House policies" and "Nearby places" fieldsets of the listing studio
//! form (spec-122 §2.6, §2.8). Policies are free text; nearby places are
//! fixed rows of name and drive time. Empty rows are ignored.

use std::collections::BTreeMap;

use common::listing_details::{
    HousePolicies, MAX_DRIVE_MINUTES, MAX_NEARBY, MAX_NEARBY_NAME_CHARS, MAX_POLICY_CHARS,
    NEARBY_KEY, NearbyPlace, POLICIES_KEY,
};
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};

/// Nearby place rows the form offers.
pub const NEARBY_SLOTS: usize = MAX_NEARBY;

/// The policy fields: (form name, label, placeholder).
pub const POLICY_FIELDS: [(&str, &str, &str); 6] = [
    ("policy_check_in", "Check-in", "From 3:00 PM"),
    ("policy_check_out", "Check-out", "By 11:00 AM"),
    (
        "policy_minimum_stay",
        "Minimum stay",
        "3 nights; 7 nights Dec 18 – Jan 4",
    ),
    (
        "policy_cancellation",
        "Cancellation",
        "Full refund up to 30 days before arrival",
    ),
    (
        "policy_security_deposit",
        "Security deposit",
        "Released 7 days after departure",
    ),
    (
        "policy_house_rules",
        "House rules",
        "No smoking indoors; quiet from 11 PM",
    ),
];

/// Form field name for nearby row `n` (from 1): `nearby_2_minutes`.
pub fn nearby_key(n: usize, part: &str) -> String {
    format!("nearby_{n}_{part}")
}

fn policy_values(p: &HousePolicies) -> [&Option<String>; 6] {
    [
        &p.check_in,
        &p.check_out,
        &p.minimum_stay,
        &p.cancellation,
        &p.security_deposit,
        &p.house_rules,
    ]
}

/// The form values for a listing's stored policies and nearby places.
pub fn policies_fields(details: Option<&serde_json::Value>) -> BTreeMap<String, String> {
    let policies: HousePolicies = details
        .and_then(|d| d.get(POLICIES_KEY))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let nearby: Vec<NearbyPlace> = details
        .and_then(|d| d.get(NEARBY_KEY))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    POLICY_FIELDS
        .iter()
        .zip(policy_values(&policies))
        .filter_map(|((name, _, _), value)| value.clone().map(|v| (name.to_string(), v)))
        .chain(
            nearby
                .into_iter()
                .take(NEARBY_SLOTS)
                .enumerate()
                .flat_map(|(i, p)| {
                    [
                        (nearby_key(i + 1, "name"), p.name),
                        (nearby_key(i + 1, "minutes"), p.drive_minutes.to_string()),
                    ]
                }),
        )
        .collect()
}

/// Policies and nearby places as posted (`None` / empty when blank), or the
/// reasons they can't be saved.
pub fn parse_policies(
    fields: &BTreeMap<String, String>,
) -> std::result::Result<(Option<HousePolicies>, Vec<NearbyPlace>), Vec<String>> {
    let get = |key: &str| {
        fields
            .get(key)
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let mut errors = Vec::new();
    let mut text = |name: &str, label: &str| {
        let v = get(name);
        if v.chars().count() > MAX_POLICY_CHARS {
            errors.push(format!(
                "{label} can be at most {MAX_POLICY_CHARS} characters."
            ));
        }
        Some(v).filter(|v| !v.is_empty())
    };
    let [
        check_in,
        check_out,
        minimum_stay,
        cancellation,
        security_deposit,
        house_rules,
    ] = POLICY_FIELDS.map(|(name, label, _)| text(name, label));
    let policies = HousePolicies {
        check_in,
        check_out,
        minimum_stay,
        cancellation,
        security_deposit,
        house_rules,
    };
    let mut nearby = Vec::new();
    for n in 1..=NEARBY_SLOTS {
        let (name, minutes) = (get(&nearby_key(n, "name")), get(&nearby_key(n, "minutes")));
        if name.is_empty() && minutes.is_empty() {
            continue;
        }
        if name.is_empty() || name.chars().count() > MAX_NEARBY_NAME_CHARS {
            errors.push(format!(
                "Nearby place {n} needs a name of up to {MAX_NEARBY_NAME_CHARS} characters."
            ));
            continue;
        }
        match minutes
            .parse::<i32>()
            .ok()
            .filter(|m| (1..=MAX_DRIVE_MINUTES).contains(m))
        {
            Some(drive_minutes) => nearby.push(NearbyPlace {
                name,
                drive_minutes,
            }),
            None => errors.push(format!(
                "Nearby place {n}'s drive time must be a whole number of minutes from 1 to {MAX_DRIVE_MINUTES}."
            )),
        }
    }
    if errors.is_empty() {
        Ok(((!policies.rows().is_empty()).then_some(policies), nearby))
    } else {
        Err(errors)
    }
}

/// The two fieldsets.
#[component]
pub async fn policies_fieldset(
    cx: &Cx,
    /// Current form values.
    fields: BTreeMap<String, String>,
    /// Class for field labels.
    label_class: &'static str,
    /// Class for inputs.
    input_class: &'static str,
) -> Result<impl View> {
    let get = |key: &str| fields.get(key).cloned().unwrap_or_default();
    let policies: Vec<(&str, &str, &str, String)> = POLICY_FIELDS
        .iter()
        .map(|(name, label, hint)| (*name, *label, *hint, get(name)))
        .collect();
    let rows: Vec<(usize, String, String)> = (1..=NEARBY_SLOTS)
        .map(|n| {
            (
                n,
                get(&nearby_key(n, "name")),
                get(&nearby_key(n, "minutes")),
            )
        })
        .collect();
    Ok(view! {
        <fieldset id="policies-editor" class="grid gap-4 rounded-xl border border-border p-6 sm:grid-cols-2">
            <legend class="px-1 text-sm font-semibold">"House policies"</legend>
            for (name, label, hint, value) in policies {
                <label class=(label_class)>
                    (label)
                    <input
                        name=(name)
                        maxlength=(MAX_POLICY_CHARS.to_string())
                        placeholder=(hint)
                        value=(value)
                        class=(input_class)
                    />
                </label>
            }
        </fieldset>
        <fieldset id="nearby-editor" class="grid gap-3 rounded-xl border border-border p-6">
            <legend class="px-1 text-sm font-semibold">"Nearby places"</legend>
            <p class="text-xs text-muted-foreground">
                "Attractions, beaches and airports with the drive time in minutes, up to 8. Guests see them under Location."
            </p>
            <div class="grid gap-2 md:grid-cols-2">
                for (n, name, minutes) in rows {
                    <div data-nearby-row=(n.to_string()) class="grid grid-cols-[1fr_6rem] gap-2">
                        <input
                            name=(nearby_key(n, "name"))
                            aria-label=(format!("Nearby place {n} name"))
                            maxlength=(MAX_NEARBY_NAME_CHARS.to_string())
                            placeholder=(if n == 1 { "Green Grotto Caves" } else { "" })
                            value=(name)
                            class=(input_class)
                        />
                        <input
                            name=(nearby_key(n, "minutes"))
                            aria-label=(format!("Nearby place {n} drive minutes"))
                            type="number"
                            min="1"
                            max=(MAX_DRIVE_MINUTES.to_string())
                            placeholder=(if n == 1 { "10" } else { "" })
                            value=(minutes)
                            class=(input_class)
                        />
                    </div>
                }
            </div>
        </fieldset>
    })
}
