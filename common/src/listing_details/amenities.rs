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
