use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UserAttribute {
    IsAdmin,
    CanManageListings,
    CanManageBookings,
}

impl UserAttribute {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserAttribute::IsAdmin => "is_admin",
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
