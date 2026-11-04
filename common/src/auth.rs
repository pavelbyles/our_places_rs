use serde::{Deserialize, Serialize};
use std::fmt;

/// A permission stored in a user's attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UserAttribute {
    /// May manage listings.
    CanManageListings,
    /// May manage bookings.
    CanManageBookings,
}

impl UserAttribute {
    /// The key it's stored under.
    pub fn as_str(&self) -> &'static str {
        match self {
            UserAttribute::CanManageListings => "can_manage_listings",
            UserAttribute::CanManageBookings => "can_manage_bookings",
        }
    }
}

impl fmt::Display for UserAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
