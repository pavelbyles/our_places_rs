//! Host-configured amenity groups (spec-122 §2.4).

use serde::{Deserialize, Serialize};

/// Icons a host may pick for an amenity group. A closed set, rendered from
/// our own SVGs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AmenityIcon {
    /// A house.
    Home,
    /// Water waves.
    Waves,
    /// A heart (wellness).
    Heart,
    /// People (family).
    Family,
    /// The sun (outdoor areas).
    Sun,
    /// Knife and fork (dining).
    Utensils,
    /// A sparkle.
    Sparkles,
    /// A shield (security).
    Shield,
    /// A car.
    Car,
    /// A star.
    Star,
}

/// A titled group of amenities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmenityGroup {
    /// Group title, e.g. "Villa".
    pub title: String,
    /// Icon beside the title.
    pub icon: AmenityIcon,
    /// Amenities in this group.
    #[serde(default)]
    pub items: Vec<String>,
}

/// Most amenities a listing can list, across all groups.
pub const MAX_AMENITIES: usize = 50;
/// Longest amenity, in characters.
pub const MAX_AMENITY_CHARS: usize = 60;
/// Most groups a listing can have.
pub const MAX_AMENITY_GROUPS: usize = 6;
/// Most items in one group.
pub const MAX_GROUP_ITEMS: usize = 20;
/// Longest group title, in characters.
pub const MAX_GROUP_TITLE_CHARS: usize = 32;

/// The groups a new listing starts with (titles only; hosts add the items).
pub const SUGGESTED_AMENITY_GROUPS: [(&str, AmenityIcon); 4] = [
    ("Villa", AmenityIcon::Home),
    ("Health & Wellness", AmenityIcon::Waves),
    ("Family", AmenityIcon::Family),
    ("Common Areas", AmenityIcon::Sun),
];

impl AmenityIcon {
    /// Every icon, in the order the admin picker lists them.
    pub const ALL: [AmenityIcon; 10] = [
        AmenityIcon::Home,
        AmenityIcon::Waves,
        AmenityIcon::Heart,
        AmenityIcon::Family,
        AmenityIcon::Sun,
        AmenityIcon::Utensils,
        AmenityIcon::Sparkles,
        AmenityIcon::Shield,
        AmenityIcon::Car,
        AmenityIcon::Star,
    ];

    /// The stored name, e.g. `"waves"`.
    pub fn key(self) -> &'static str {
        match self {
            AmenityIcon::Home => "home",
            AmenityIcon::Waves => "waves",
            AmenityIcon::Heart => "heart",
            AmenityIcon::Family => "family",
            AmenityIcon::Sun => "sun",
            AmenityIcon::Utensils => "utensils",
            AmenityIcon::Sparkles => "sparkles",
            AmenityIcon::Shield => "shield",
            AmenityIcon::Car => "car",
            AmenityIcon::Star => "star",
        }
    }

    /// The name hosts see in the picker.
    pub fn label(self) -> &'static str {
        match self {
            AmenityIcon::Home => "Home",
            AmenityIcon::Waves => "Water",
            AmenityIcon::Heart => "Wellness",
            AmenityIcon::Family => "Family",
            AmenityIcon::Sun => "Outdoors",
            AmenityIcon::Utensils => "Dining",
            AmenityIcon::Sparkles => "Extras",
            AmenityIcon::Shield => "Security",
            AmenityIcon::Car => "Transport",
            AmenityIcon::Star => "Star",
        }
    }

    /// The icon stored as `key`, if any.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.key() == key)
    }
}

/// Why amenity groups can't be saved. Group numbers count from 1.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum AmenityGroupError {
    /// More than 6 groups.
    #[error("Add at most 6 amenity groups, got {0}.")]
    Count(usize),
    /// A group has no title.
    #[error("Group {0} needs a title.")]
    MissingTitle(usize),
    /// A group's title is over 32 characters.
    #[error("Group {0}'s title is longer than 32 characters.")]
    TitleTooLong(usize),
    /// A group has no items, or more than 20.
    #[error("Group {0} needs between 1 and 20 items.")]
    ItemCount(usize),
    /// An item is over 60 characters.
    #[error("An item in group {0} is longer than 60 characters.")]
    ItemTooLong(usize),
    /// More than 50 items across all groups.
    #[error("List at most 50 amenities in total, got {0}.")]
    TotalItems(usize),
}

/// Checks groups as saved. An empty list is allowed (no amenities section).
pub fn validate_amenity_groups(groups: &[AmenityGroup]) -> Result<(), AmenityGroupError> {
    if groups.len() > MAX_AMENITY_GROUPS {
        return Err(AmenityGroupError::Count(groups.len()));
    }
    groups.iter().enumerate().try_for_each(|(i, g)| {
        let n = i + 1;
        let title = g.title.trim().chars().count();
        if title == 0 {
            Err(AmenityGroupError::MissingTitle(n))
        } else if title > MAX_GROUP_TITLE_CHARS {
            Err(AmenityGroupError::TitleTooLong(n))
        } else if g.items.is_empty() || g.items.len() > MAX_GROUP_ITEMS {
            Err(AmenityGroupError::ItemCount(n))
        } else if g
            .items
            .iter()
            .any(|item| item.chars().count() > MAX_AMENITY_CHARS)
        {
            Err(AmenityGroupError::ItemTooLong(n))
        } else {
            Ok(())
        }
    })?;
    let total: usize = groups.iter().map(|g| g.items.len()).sum();
    if total > MAX_AMENITIES {
        return Err(AmenityGroupError::TotalItems(total));
    }
    Ok(())
}

/// The groups to show: the host's groups when set, else a legacy flat list
/// as one "Amenities" group, else none. Groups without items are dropped.
pub fn amenity_groups_for(groups: Option<&[AmenityGroup]>, legacy: &[String]) -> Vec<AmenityGroup> {
    match groups {
        Some(groups) => groups
            .iter()
            .filter(|g| !g.items.is_empty())
            .cloned()
            .collect(),
        None if !legacy.is_empty() => vec![AmenityGroup {
            title: "Amenities".to_string(),
            icon: AmenityIcon::Home,
            items: legacy.to_vec(),
        }],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(title: &str, items: usize) -> AmenityGroup {
        AmenityGroup {
            title: title.into(),
            icon: AmenityIcon::Home,
            items: (0..items).map(|i| format!("Item {i}")).collect(),
        }
    }

    #[test]
    fn group_count_is_capped_at_six() {
        assert_eq!(validate_amenity_groups(&[]), Ok(()));
        assert_eq!(validate_amenity_groups(&[group("Villa", 1)]), Ok(()));
        let six: Vec<_> = (0..6).map(|_| group("G", 1)).collect();
        assert_eq!(validate_amenity_groups(&six), Ok(()));
        let seven: Vec<_> = (0..7).map(|_| group("G", 1)).collect();
        assert_eq!(
            validate_amenity_groups(&seven),
            Err(AmenityGroupError::Count(7))
        );
    }

    #[test]
    fn titles_are_required_and_capped() {
        assert_eq!(
            validate_amenity_groups(&[group("Villa", 1), group("  ", 1)]),
            Err(AmenityGroupError::MissingTitle(2))
        );
        assert_eq!(
            validate_amenity_groups(&[group(&"x".repeat(32), 1)]),
            Ok(())
        );
        assert_eq!(
            validate_amenity_groups(&[group(&"x".repeat(33), 1)]),
            Err(AmenityGroupError::TitleTooLong(1))
        );
    }

    #[test]
    fn items_per_group_and_in_total_are_capped() {
        assert_eq!(
            validate_amenity_groups(&[group("Villa", 0)]),
            Err(AmenityGroupError::ItemCount(1))
        );
        assert_eq!(validate_amenity_groups(&[group("Villa", 20)]), Ok(()));
        assert_eq!(
            validate_amenity_groups(&[group("Villa", 21)]),
            Err(AmenityGroupError::ItemCount(1))
        );
        let fifty: Vec<_> = (0..5).map(|_| group("G", 10)).collect();
        assert_eq!(validate_amenity_groups(&fifty), Ok(()));
        let mut fifty_one = fifty.clone();
        fifty_one[0].items.push("One more".into());
        assert_eq!(
            validate_amenity_groups(&fifty_one),
            Err(AmenityGroupError::TotalItems(51))
        );
    }

    #[test]
    fn long_items_are_refused() {
        let mut g = group("Villa", 1);
        g.items[0] = "x".repeat(60);
        assert_eq!(validate_amenity_groups(std::slice::from_ref(&g)), Ok(()));
        g.items[0] = "x".repeat(61);
        assert_eq!(
            validate_amenity_groups(&[g]),
            Err(AmenityGroupError::ItemTooLong(1))
        );
    }

    #[test]
    fn legacy_flat_lists_become_one_group() {
        let legacy = vec!["Pool".to_string(), "Wi-Fi".to_string()];
        let shown = amenity_groups_for(None, &legacy);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].title, "Amenities");
        assert_eq!(shown[0].items, legacy);
        assert!(amenity_groups_for(None, &[]).is_empty());
        let groups = vec![group("Villa", 2), group("Empty", 0)];
        assert_eq!(
            amenity_groups_for(Some(&groups), &legacy).len(),
            1,
            "host groups win; empty ones drop"
        );
    }

    #[test]
    fn icon_keys_match_their_stored_names() {
        for icon in AmenityIcon::ALL {
            assert_eq!(
                serde_json::to_value(icon).ok(),
                Some(serde_json::json!(icon.key()))
            );
            assert_eq!(AmenityIcon::from_key(icon.key()), Some(icon));
        }
        for (_, icon) in SUGGESTED_AMENITY_GROUPS {
            assert!(AmenityIcon::ALL.contains(&icon));
        }
    }
}
