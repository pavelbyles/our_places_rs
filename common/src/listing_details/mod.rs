//! Typed view of a listing's `listing_details` JSONB (spec-122 §3.1).
//!
//! Hosts edit these sections in the admin listing form; the guest listing
//! page renders them. Every section is optional and parsed on its own, so a
//! malformed section is dropped (with a warning) without hiding the rest.

pub mod amenities;
pub mod ribbon;

use rust_decimal::Decimal;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub use amenities::{
    AmenityGroup, AmenityGroupError, AmenityIcon, SUGGESTED_AMENITY_GROUPS, amenity_groups_for,
    validate_amenity_groups,
};
pub use ribbon::{Ribbon, RibbonError, RibbonIcon, RibbonItem};

/// `listing_details` key: the italic pull-quote above the description.
pub const PULL_QUOTE_KEY: &str = "pull_quote";
/// `listing_details` key: the host-entered highlights ribbon.
pub const RIBBON_KEY: &str = "ribbon";
/// `listing_details` key: the room-by-room suites.
pub const SUITES_KEY: &str = "suites";
/// `listing_details` key: the host-configured amenity groups.
pub const AMENITY_GROUPS_KEY: &str = "amenity_groups";
/// `listing_details` key: the legacy flat amenity list.
pub const AMENITIES_KEY: &str = "amenities";
/// `listing_details` key: included staff and paid add-ons.
pub const STAFF_SERVICES_KEY: &str = "staff_services";
/// `listing_details` key: house policies.
pub const POLICIES_KEY: &str = "policies";
/// `listing_details` key: nearby places with drive times.
pub const NEARBY_KEY: &str = "nearby";
/// `listing_details` key: guestbook quotes shown when there are no reviews.
pub const TESTIMONIALS_KEY: &str = "curated_testimonials";

/// One bedroom or suite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuiteItem {
    /// Suite name, e.g. "Oceanfront Master".
    pub name: String,
    /// Bed configuration, e.g. "1 king".
    pub bed_configuration: String,
    /// Most guests who can sleep here.
    pub max_guests: i32,
    /// Chips such as "Direct ocean view" or "Private balcony".
    #[serde(default)]
    pub features: Vec<String>,
    /// En-suite description, e.g. "En-suite, soaking tub"; `None` means a shared bath.
    #[serde(default)]
    pub en_suite: Option<String>,
    /// One of the listing's own uploaded images.
    #[serde(default)]
    pub image_id: Option<Uuid>,
}

/// An optional paid service listed under "Concierge add-ons".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceAddon {
    /// Service name, e.g. "Airport transfer, MBJ".
    pub name: String,
    /// Price in the listing's base currency; `None` shows "On request".
    #[serde(default)]
    pub price: Option<Decimal>,
    /// What the price is per, e.g. "each way", "day".
    #[serde(default)]
    pub unit: Option<String>,
}

/// Staff included with the stay, and paid add-ons.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StaffServices {
    /// Included staff and services, e.g. "Private chef".
    #[serde(default)]
    pub included: Vec<String>,
    /// A note under the included list, e.g. gratuity guidance.
    #[serde(default)]
    pub included_note: Option<String>,
    /// Paid add-ons.
    #[serde(default)]
    pub addons: Vec<ServiceAddon>,
}

impl StaffServices {
    /// Whether there is anything to show.
    pub fn is_empty(&self) -> bool {
        self.included.is_empty() && self.addons.is_empty()
    }
}

/// House policies shown before booking. Free text per row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HousePolicies {
    /// E.g. "From 3:00 PM".
    #[serde(default)]
    pub check_in: Option<String>,
    /// E.g. "By 11:00 AM".
    #[serde(default)]
    pub check_out: Option<String>,
    /// E.g. "3 nights · 7 nights Dec 18 – Jan 4".
    #[serde(default)]
    pub minimum_stay: Option<String>,
    /// E.g. "Full refund up to 30 days before arrival".
    #[serde(default)]
    pub cancellation: Option<String>,
    /// E.g. "$1,500, released 7 days after departure".
    #[serde(default)]
    pub security_deposit: Option<String>,
    /// E.g. "No smoking indoors · quiet from 11 PM".
    #[serde(default)]
    pub house_rules: Option<String>,
}

impl HousePolicies {
    /// The non-empty rows as (label, text), in display order.
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        [
            ("Check-in", &self.check_in),
            ("Check-out", &self.check_out),
            ("Minimum stay", &self.minimum_stay),
            ("Cancellation", &self.cancellation),
            ("Security deposit", &self.security_deposit),
            ("House rules", &self.house_rules),
        ]
        .into_iter()
        .filter_map(|(label, text)| {
            text.as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(|t| (label, t.to_string()))
        })
        .collect()
    }
}

/// A nearby place with its drive time from the listing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NearbyPlace {
    /// E.g. "Green Grotto Caves".
    pub name: String,
    /// Drive time in minutes.
    pub drive_minutes: i32,
}

/// "10 min", "1h", "1h 15m". Negative values read as 0.
pub fn drive_time(minutes: i32) -> String {
    let m = minutes.max(0);
    match (m / 60, m % 60) {
        (0, mins) => format!("{mins} min"),
        (hours, 0) => format!("{hours}h"),
        (hours, mins) => format!("{hours}h {mins}m"),
    }
}

/// A guestbook quote, shown only when there are no verified reviews.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CuratedTestimonial {
    /// Guest's first name.
    pub guest_name: String,
    /// Guest's home city.
    #[serde(default)]
    pub location: Option<String>,
    /// When they stayed, e.g. "March 2026".
    pub stay_date: String,
    /// The quote.
    pub quote: String,
    /// 1–5 stars.
    pub rating: i32,
}

/// Everything structured in `listing_details`. Unknown keys are ignored.
/// (Phase 2 adds experiences.)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructuredListingDetails {
    /// Italic pull-quote above the description.
    pub pull_quote: Option<String>,
    /// Host-entered ribbon items, unvalidated; see [`ribbon::Ribbon::for_listing`].
    pub ribbon: Option<Vec<RibbonItem>>,
    /// Suites in host order.
    pub suites: Vec<SuiteItem>,
    /// Host-configured amenity groups.
    pub amenity_groups: Option<Vec<AmenityGroup>>,
    /// Legacy flat amenity list.
    pub amenities: Vec<String>,
    /// Staff and add-ons.
    pub staff_services: StaffServices,
    /// House policies.
    pub policies: HousePolicies,
    /// Nearby places.
    pub nearby: Vec<NearbyPlace>,
    /// Guestbook quotes.
    pub curated_testimonials: Vec<CuratedTestimonial>,
}

/// Parses one section; a missing or `null` key is `None`, a malformed one is
/// logged and treated as missing.
fn section<T: DeserializeOwned>(details: &Value, key: &str) -> Option<T> {
    details
        .get(key)
        .filter(|v| !v.is_null())
        .and_then(|v| {
            serde_json::from_value::<T>(v.clone())
                .map_err(|e| {
                    tracing::warn!(section = key, error = %e, "ignoring malformed listing_details section");
                })
                .ok()
        })
}

impl StructuredListingDetails {
    /// Reads a listing's `listing_details`, section by section.
    pub fn from_json(details: Option<&Value>) -> Self {
        let Some(d) = details.filter(|d| d.is_object()) else {
            return Self::default();
        };
        Self {
            pull_quote: section::<String>(d, PULL_QUOTE_KEY)
                .map(|q| q.trim().to_string())
                .filter(|q| !q.is_empty()),
            ribbon: section(d, RIBBON_KEY),
            suites: section(d, SUITES_KEY).unwrap_or_default(),
            amenity_groups: section(d, AMENITY_GROUPS_KEY),
            amenities: section(d, AMENITIES_KEY).unwrap_or_default(),
            staff_services: section(d, STAFF_SERVICES_KEY).unwrap_or_default(),
            policies: section(d, POLICIES_KEY).unwrap_or_default(),
            nearby: section(d, NEARBY_KEY).unwrap_or_default(),
            curated_testimonials: section(d, TESTIMONIALS_KEY).unwrap_or_default(),
        }
    }
}

fn invalid(code: &'static str, message: String) -> validator::ValidationError {
    validator::ValidationError::new(code).with_message(message.into())
}

/// The ribbon section, if present, must be 3–6 valid items. An empty list
/// means "not set" and is allowed.
fn validate_ribbon_section(details: &Value) -> Result<(), validator::ValidationError> {
    details
        .get(RIBBON_KEY)
        .filter(|v| !v.is_null())
        .map_or(Ok(()), |v| {
            serde_json::from_value::<Vec<RibbonItem>>(v.clone())
                .map_err(|_| {
                    invalid(
                        "INVALID_RIBBON",
                        "Each ribbon item needs a known icon, a value and a label.".to_string(),
                    )
                })
                .and_then(|items| {
                    if items.is_empty() {
                        Ok(())
                    } else {
                        Ribbon::try_from(items)
                            .map(|_| ())
                            .map_err(|e| invalid("INVALID_RIBBON", e.to_string()))
                    }
                })
        })
}

/// The amenity groups section, if present, must pass
/// [`validate_amenity_groups`].
fn validate_amenity_groups_section(details: &Value) -> Result<(), validator::ValidationError> {
    details
        .get(AMENITY_GROUPS_KEY)
        .filter(|v| !v.is_null())
        .map_or(Ok(()), |v| {
            serde_json::from_value::<Vec<AmenityGroup>>(v.clone())
                .map_err(|_| {
                    invalid(
                        "INVALID_AMENITY_GROUPS",
                        "Each amenity group needs a title, a known icon and a list of items."
                            .to_string(),
                    )
                })
                .and_then(|groups| {
                    validate_amenity_groups(&groups)
                        .map_err(|e| invalid("INVALID_AMENITY_GROUPS", e.to_string()))
                })
        })
}

/// Validates the host-edited sections of `listing_details` on create and
/// update. Error codes: `INVALID_RIBBON`, `INVALID_AMENITY_GROUPS`.
pub fn validate_listing_details(details: &Value) -> Result<(), validator::ValidationError> {
    validate_ribbon_section(details).and_then(|()| validate_amenity_groups_section(details))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ribbon_items(n: usize) -> Value {
        Value::Array(
            (0..n)
                .map(|i| json!({ "icon": "bed", "value": i.to_string(), "label": "Bedrooms" }))
                .collect(),
        )
    }

    #[test]
    fn listing_details_without_a_ribbon_are_valid() {
        assert!(validate_listing_details(&json!({ "amenities": ["Pool"] })).is_ok());
        assert!(validate_listing_details(&json!({ "ribbon": null })).is_ok());
        assert!(validate_listing_details(&json!({ "ribbon": [] })).is_ok());
    }

    #[test]
    fn invalid_amenity_groups_are_refused_with_their_reason() {
        let untitled = json!({ "amenity_groups": [
            { "title": "Villa", "icon": "home", "items": ["Pool"] },
            { "title": "", "icon": "sun", "items": ["Gazebo"] },
        ]});
        let e = validate_listing_details(&untitled).err();
        assert_eq!(
            e.as_ref().map(|e| e.code.to_string()),
            Some("INVALID_AMENITY_GROUPS".into())
        );
        assert_eq!(
            e.and_then(|e| e.message).map(|m| m.to_string()),
            Some("Group 2 needs a title.".to_string())
        );
        let valid =
            json!({ "amenity_groups": [{ "title": "Villa", "icon": "home", "items": ["Pool"] }] });
        assert!(validate_listing_details(&valid).is_ok());
        assert!(validate_listing_details(&json!({ "amenity_groups": [] })).is_ok());
    }

    #[test]
    fn an_invalid_ribbon_is_refused_with_its_reason() {
        let e = validate_listing_details(&json!({ "ribbon": ribbon_items(2) })).err();
        assert_eq!(e.as_ref().map(|e| e.code.as_ref()), Some("INVALID_RIBBON"));
        assert_eq!(
            e.and_then(|e| e.message).map(|m| m.to_string()),
            Some("A ribbon needs between 3 and 6 items, got 2.".to_string())
        );
        let bad_icon = json!({ "ribbon": [{ "icon": "rocket", "value": "1", "label": "x" }] });
        assert_eq!(
            validate_listing_details(&bad_icon)
                .err()
                .map(|e| e.code.to_string()),
            Some("INVALID_RIBBON".to_string())
        );
        assert!(validate_listing_details(&json!({ "ribbon": ribbon_items(3) })).is_ok());
    }

    #[test]
    fn nothing_stored_is_the_default() {
        assert_eq!(
            StructuredListingDetails::from_json(None),
            StructuredListingDetails::default()
        );
        assert_eq!(
            StructuredListingDetails::from_json(Some(&json!({}))),
            StructuredListingDetails::default()
        );
        assert_eq!(
            StructuredListingDetails::from_json(Some(&json!("not an object"))),
            StructuredListingDetails::default()
        );
    }

    #[test]
    fn a_malformed_section_does_not_hide_the_others() {
        let d = json!({
            "suites": [{ "name": "Master" }],
            "nearby": [{ "name": "Green Grotto Caves", "drive_minutes": 10 }],
            "pull_quote": "  A private beachfront sanctuary  ",
        });
        let parsed = StructuredListingDetails::from_json(Some(&d));
        assert!(
            parsed.suites.is_empty(),
            "suite without required fields is dropped"
        );
        assert_eq!(parsed.nearby.len(), 1);
        assert_eq!(
            parsed.pull_quote.as_deref(),
            Some("A private beachfront sanctuary")
        );
    }

    #[test]
    fn unknown_keys_and_legacy_amenities_are_read() {
        let d = json!({ "amenities": ["Pool", "Wi-Fi"], "future_key": { "x": 1 } });
        let parsed = StructuredListingDetails::from_json(Some(&d));
        assert_eq!(parsed.amenities, ["Pool", "Wi-Fi"]);
        assert!(parsed.amenity_groups.is_none());
    }

    #[test]
    fn full_sections_round_trip() {
        let d = json!({
            "suites": [{
                "name": "Oceanfront Master", "bed_configuration": "1 king", "max_guests": 2,
                "features": ["Direct ocean view"], "en_suite": "En-suite, soaking tub"
            }],
            "staff_services": {
                "included": ["Private chef"],
                "addons": [{ "name": "Airport transfer", "price": "180", "unit": "each way" },
                           { "name": "Groceries" }]
            },
            "policies": { "check_in": "From 3:00 PM", "house_rules": "  " },
            "curated_testimonials": [{
                "guest_name": "Danielle", "stay_date": "March 2026", "quote": "Lovely", "rating": 5
            }]
        });
        let parsed = StructuredListingDetails::from_json(Some(&d));
        assert_eq!(
            parsed.suites[0].en_suite.as_deref(),
            Some("En-suite, soaking tub")
        );
        assert_eq!(
            parsed.staff_services.addons[0].price,
            Some(Decimal::new(180, 0))
        );
        assert_eq!(parsed.staff_services.addons[1].price, None);
        assert_eq!(
            parsed.policies.rows(),
            [("Check-in", "From 3:00 PM".to_string())]
        );
        assert_eq!(parsed.curated_testimonials[0].rating, 5);
    }

    #[test]
    fn drive_times_read_naturally() {
        assert_eq!(drive_time(0), "0 min");
        assert_eq!(drive_time(10), "10 min");
        assert_eq!(drive_time(59), "59 min");
        assert_eq!(drive_time(60), "1h");
        assert_eq!(drive_time(75), "1h 15m");
        assert_eq!(drive_time(120), "2h");
        assert_eq!(drive_time(-5), "0 min");
    }
}
