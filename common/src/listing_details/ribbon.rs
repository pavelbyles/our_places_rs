//! The host-entered highlights ribbon (spec-122 §2.2).

use serde::{Deserialize, Serialize};

/// Icons a host may pick for a ribbon item. A closed set, rendered from our
/// own SVGs, so hosts never supply markup.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RibbonIcon {
    /// A bed.
    Bed,
    /// A bathtub.
    Bath,
    /// People.
    Guests,
    /// Water waves (pool).
    Pool,
    /// A house (beach house, beach access).
    Beach,
    /// A sunrise over the sea.
    Ocean,
    /// A chef's hat.
    Chef,
    /// A concierge bell.
    Butler,
    /// A shield.
    Security,
    /// Wi-Fi.
    Wifi,
    /// The sun (solar power).
    Solar,
    /// A car.
    Car,
    /// A dumbbell.
    Gym,
    /// A star.
    Star,
}

/// One ribbon item: an icon, a short value and a label ("6" / "Bedrooms").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RibbonItem {
    /// Icon above the value.
    pub icon: RibbonIcon,
    /// Short value, e.g. "6", "5.5", "Direct".
    pub value: String,
    /// Label under the value, e.g. "Bedrooms".
    pub label: String,
}

/// Fewest ribbon items a host may save.
pub const MIN_RIBBON_ITEMS: usize = 3;
/// Most ribbon items a host may save.
pub const MAX_RIBBON_ITEMS: usize = 6;
/// Longest ribbon value, in characters.
pub const MAX_RIBBON_VALUE_CHARS: usize = 8;
/// Longest ribbon label, in characters.
pub const MAX_RIBBON_LABEL_CHARS: usize = 24;

/// Why a ribbon can't be saved. Item numbers count from 1.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum RibbonError {
    /// Fewer than 3 or more than 6 items.
    #[error("A ribbon needs between 3 and 6 items, got {0}.")]
    Count(usize),
    /// An item has a blank value or label.
    #[error("Item {0} needs a value and a label.")]
    Incomplete(usize),
    /// An item's value is over 8 characters.
    #[error("Item {0}'s value is longer than 8 characters.")]
    ValueTooLong(usize),
    /// An item's label is over 24 characters.
    #[error("Item {0}'s label is longer than 24 characters.")]
    LabelTooLong(usize),
}

/// A valid ribbon: 3–6 complete items, trimmed. Build it with
/// [`Ribbon::try_from`], [`Ribbon::fallback`] or [`Ribbon::resolve`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Ribbon(Vec<RibbonItem>);

impl TryFrom<Vec<RibbonItem>> for Ribbon {
    type Error = RibbonError;

    fn try_from(items: Vec<RibbonItem>) -> Result<Self, Self::Error> {
        if !(MIN_RIBBON_ITEMS..=MAX_RIBBON_ITEMS).contains(&items.len()) {
            return Err(RibbonError::Count(items.len()));
        }
        items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let n = i + 1;
                let value = item.value.trim().to_string();
                let label = item.label.trim().to_string();
                match (value.chars().count(), label.chars().count()) {
                    (0, _) | (_, 0) => Err(RibbonError::Incomplete(n)),
                    (v, _) if v > MAX_RIBBON_VALUE_CHARS => Err(RibbonError::ValueTooLong(n)),
                    (_, l) if l > MAX_RIBBON_LABEL_CHARS => Err(RibbonError::LabelTooLong(n)),
                    _ => Ok(RibbonItem {
                        icon: item.icon,
                        value,
                        label,
                    }),
                }
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Ribbon)
    }
}

/// `full + ½ × half` bathrooms, without trailing zeros: 5 + 1 → "5.5", 5 + 2 → "6".
fn bathrooms_value(full: i32, half: i32) -> String {
    (rust_decimal::Decimal::from(full.max(0))
        + rust_decimal::Decimal::new(i64::from(half.max(0)) * 5, 1))
    .normalize()
    .to_string()
}

impl Ribbon {
    /// The items, in display order.
    pub fn items(&self) -> &[RibbonItem] {
        &self.0
    }

    /// The ribbon a listing gets when its host hasn't set one: bedrooms,
    /// bathrooms and maximum guests.
    pub fn fallback(bedrooms: i32, full_baths: i32, half_baths: i32, max_guests: i32) -> Self {
        let item = |icon, value: String, label: &str| RibbonItem {
            icon,
            value,
            label: label.to_string(),
        };
        Ribbon(vec![
            item(RibbonIcon::Bed, bedrooms.max(0).to_string(), "Bedrooms"),
            item(
                RibbonIcon::Bath,
                bathrooms_value(full_baths, half_baths),
                "Bathrooms",
            ),
            item(
                RibbonIcon::Guests,
                max_guests.max(0).to_string(),
                "Max guests",
            ),
        ])
    }

    /// The host's ribbon when it is valid, else `fallback`. Invalid stored
    /// items are logged, never shown.
    pub fn resolve(stored: Option<Vec<RibbonItem>>, fallback: Ribbon) -> Self {
        match stored
            .filter(|items| !items.is_empty())
            .map(Ribbon::try_from)
        {
            Some(Ok(ribbon)) => ribbon,
            Some(Err(e)) => {
                tracing::warn!(error = %e, "stored ribbon is invalid; showing the listing facts instead");
                fallback
            }
            None => fallback,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(value: &str, label: &str) -> RibbonItem {
        RibbonItem {
            icon: RibbonIcon::Star,
            value: value.into(),
            label: label.into(),
        }
    }

    fn items(n: usize) -> Vec<RibbonItem> {
        (0..n).map(|i| item(&i.to_string(), "Label")).collect()
    }

    #[test]
    fn needs_three_to_six_items() {
        assert_eq!(Ribbon::try_from(items(2)), Err(RibbonError::Count(2)));
        assert_eq!(Ribbon::try_from(items(7)), Err(RibbonError::Count(7)));
        assert_eq!(Ribbon::try_from(items(3)).map(|r| r.items().len()), Ok(3));
        assert_eq!(Ribbon::try_from(items(6)).map(|r| r.items().len()), Ok(6));
    }

    #[test]
    fn every_item_needs_a_value_and_a_label() {
        let mut v = items(3);
        v[1] = item("  ", "Bedrooms");
        assert_eq!(Ribbon::try_from(v), Err(RibbonError::Incomplete(2)));
        let mut v = items(3);
        v[2] = item("6", "");
        assert_eq!(Ribbon::try_from(v), Err(RibbonError::Incomplete(3)));
    }

    #[test]
    fn values_and_labels_have_length_caps_in_characters() {
        let mut v = items(3);
        v[0] = item("12345678", "Exactly twenty-four char");
        assert!(Ribbon::try_from(v).is_ok());
        let mut v = items(3);
        v[0] = item("123456789", "Label");
        assert_eq!(Ribbon::try_from(v), Err(RibbonError::ValueTooLong(1)));
        let mut v = items(3);
        v[0] = item("6", "Exactly twenty-five chars");
        assert_eq!(Ribbon::try_from(v), Err(RibbonError::LabelTooLong(1)));
        let mut v = items(3);
        v[0] = item("Ñoño ñoñ", "Label");
        assert!(Ribbon::try_from(v).is_ok(), "8 characters, more bytes");
    }

    #[test]
    fn saved_items_are_trimmed() {
        let mut v = items(3);
        v[0] = item(" Direct ", " Beach access ");
        let r = Ribbon::try_from(v).map(|r| r.items()[0].clone());
        assert_eq!(
            r.map(|i| (i.value, i.label)),
            Ok(("Direct".into(), "Beach access".into()))
        );
    }

    #[test]
    fn fallback_shows_bedrooms_bathrooms_and_guests() {
        let r = Ribbon::fallback(6, 5, 1, 14);
        let shown: Vec<_> = r
            .items()
            .iter()
            .map(|i| (i.value.as_str(), i.label.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("6", "Bedrooms"),
                ("5.5", "Bathrooms"),
                ("14", "Max guests")
            ]
        );
        assert_eq!(Ribbon::fallback(3, 5, 2, 6).items()[1].value, "6");
        assert_eq!(Ribbon::fallback(1, 1, 0, 2).items()[1].value, "1");
    }

    #[test]
    fn resolve_prefers_a_valid_host_ribbon() {
        let fallback = Ribbon::fallback(2, 2, 0, 4);
        assert_eq!(
            Ribbon::resolve(Some(items(4)), fallback.clone())
                .items()
                .len(),
            4
        );
        assert_eq!(Ribbon::resolve(Some(items(2)), fallback.clone()), fallback);
        assert_eq!(Ribbon::resolve(Some(vec![]), fallback.clone()), fallback);
        assert_eq!(Ribbon::resolve(None, fallback.clone()), fallback);
    }

    #[test]
    fn unknown_icons_fail_to_parse() {
        let json = serde_json::json!([{ "icon": "rocket", "value": "1", "label": "x" }]);
        assert!(serde_json::from_value::<Vec<RibbonItem>>(json).is_err());
        let json =
            serde_json::json!({ "icon": "beach", "value": "Direct", "label": "Beach access" });
        assert!(serde_json::from_value::<RibbonItem>(json).is_ok());
    }
}
