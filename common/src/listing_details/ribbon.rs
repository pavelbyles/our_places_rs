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
