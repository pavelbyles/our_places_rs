//! The listing studio form shared by create, clone and edit.

use std::str::FromStr;

use common::booking_request::BookingMode;
use common::location::{ListingLocationResponse, LocationPrivacy};
use common::models::{ListingResponse, NewListingRequest, UpdatedListingRequest};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::BTreeMap;
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};
use uuid::Uuid;

use common::listing_details::{
    AMENITIES_KEY, AMENITY_GROUPS_KEY, AmenityGroup, CuratedTestimonial, HousePolicies, NEARBY_KEY,
    NearbyPlace, POLICIES_KEY, RIBBON_KEY, RibbonItem, STAFF_SERVICES_KEY, SUITES_KEY,
    StaffServices, SuiteItem, TESTIMONIALS_KEY,
};
use web_ui_common::components::domain::currency_selector::currency_option_list;

use super::amenity_field::{
    amenity_fields, amenity_fieldset, parse_amenity_groups, suggested_amenity_fields,
};
use super::guestbook_field::{guestbook_fields, guestbook_fieldset, parse_guestbook};
use super::policies_field::{parse_policies, policies_fields, policies_fieldset};
use super::ribbon_field::{parse_ribbon, ribbon_fields, ribbon_fieldset};
use super::services_field::{parse_services, services_fields, services_fieldset};
use super::suite_field::{SuitePhoto, parse_suites, suite_fields, suite_fieldset};

/// Listing structures known to `listing_api`.
pub const STRUCTURES: [&str; 5] = ["Villa", "House", "Apartment", "Townhouse", "Studio"];
pub use common::listing_details::amenities::{MAX_AMENITIES, MAX_AMENITY_CHARS};
/// Longest description `listing_api` accepts, in characters.
pub const MAX_DESCRIPTION_CHARS: usize = 2000;
/// Raw form values (strings, so the page can be re-rendered as typed).
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct ListingFields {
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Structure (Villa, House, …).
    pub listing_structure: String,
    /// Country.
    pub country: String,
    /// City.
    pub city: String,
    /// Base currency.
    pub base_currency: String,
    /// Nightly price.
    pub price_per_night: String,
    /// Cleaning fee.
    pub cleaning_fee: String,
    /// Refundable damage deposit.
    pub damage_deposit: String,
    /// Fee per extra guest per night.
    pub extra_guest_fee: String,
    /// Weekly discount, percent.
    pub weekly_discount_percentage: String,
    /// Monthly discount, percent.
    pub monthly_discount_percentage: String,
    /// Platform commission (admins only), fraction e.g. 0.10.
    pub commission_pct: String,
    /// Maximum guests.
    pub max_guests: String,
    /// Guests included in the nightly price.
    pub guests_included: String,
    /// Bedrooms.
    pub bedrooms: String,
    /// Beds.
    pub beds: String,
    /// Full bathrooms.
    pub full_bathrooms: String,
    /// Half bathrooms.
    pub half_bathrooms: String,
    /// Floor area.
    pub square_meters: String,
    /// Minimum nights.
    pub minimum_stay: String,
    /// Turnover days between bookings.
    pub days_between_bookings: String,
    /// Latitude.
    pub latitude: String,
    /// Longitude.
    pub longitude: String,
    /// Owner (admins choose; hosts always own what they create).
    pub owner_id: String,
    /// "on" when the listing is bookable.
    pub is_active: String,
    /// How the listing sells: `instant`, `request` or `both`.
    pub booking_mode: String,
    /// What guests see of the location before booking: `approximate` or `exact`.
    #[serde(default)]
    pub location_privacy: String,
    /// Fixed-slot content fields posted as `ribbon_{n}_{part}` and similar
    /// (the form encoding has no lists, so each row has its own names).
    #[serde(flatten)]
    pub slots: BTreeMap<String, String>,
}

fn opt(v: Option<Decimal>) -> String {
    v.map(|d| d.normalize().to_string()).unwrap_or_default()
}

impl ListingFields {
    /// Prefills the form from an existing listing.
    pub fn from_listing(l: &ListingResponse) -> Self {
        Self {
            name: l.name.clone(),
            description: l.description.clone().unwrap_or_default(),
            listing_structure: l.listing_structure.clone(),
            country: l.country.clone(),
            city: l.city.clone().unwrap_or_default(),
            base_currency: l.base_currency.clone(),
            price_per_night: opt(l.price_per_night),
            cleaning_fee: opt(l.cleaning_fee),
            damage_deposit: opt(l.damage_deposit),
            extra_guest_fee: opt(l.extra_guest_fee),
            weekly_discount_percentage: opt(l.weekly_discount_percentage),
            monthly_discount_percentage: opt(l.monthly_discount_percentage),
            commission_pct: opt(l.commission_pct),
            max_guests: l.max_guests.to_string(),
            guests_included: l.guests_included.to_string(),
            bedrooms: l.bedrooms.to_string(),
            beds: l.beds.to_string(),
            full_bathrooms: l.full_bathrooms.to_string(),
            half_bathrooms: l.half_bathrooms.to_string(),
            square_meters: l.square_meters.map(|v| v.to_string()).unwrap_or_default(),
            minimum_stay: l.minimum_stay.to_string(),
            days_between_bookings: l.days_between_bookings.to_string(),
            // Never from a listing response: on public paths it carries the
            // approximate point, and saving that would replace the stored one
            // (spec 103 §1.9). `with_location` fills these from the manager
            // endpoint.
            latitude: String::new(),
            longitude: String::new(),
            location_privacy: String::new(),
            owner_id: l.user_id.to_string(),
            is_active: if l.is_active {
                "on".into()
            } else {
                String::new()
            },
            booking_mode: l.booking_mode.as_str().into(),
            slots: ribbon_fields(l.listing_details.as_ref())
                .into_iter()
                .chain(amenity_fields(l.listing_details.as_ref()))
                .chain(suite_fields(l.listing_details.as_ref()))
                .chain(services_fields(l.listing_details.as_ref()))
                .chain(policies_fields(l.listing_details.as_ref()))
                .chain(guestbook_fields(l.listing_details.as_ref()))
                .collect(),
        }
    }

    /// Fills the location from the manager endpoint's stored values.
    pub fn with_location(self, location: &ListingLocationResponse) -> Self {
        Self {
            latitude: location.latitude.map(|v| v.to_string()).unwrap_or_default(),
            longitude: location
                .longitude
                .map(|v| v.to_string())
                .unwrap_or_default(),
            location_privacy: location.location_privacy.as_str().into(),
            ..self
        }
    }

    /// Defaults for a brand-new listing.
    pub fn blank(owner: Uuid) -> Self {
        Self {
            listing_structure: "Villa".into(),
            country: "Jamaica".into(),
            base_currency: "USD".into(),
            max_guests: "2".into(),
            guests_included: "2".into(),
            bedrooms: "1".into(),
            beds: "1".into(),
            full_bathrooms: "1".into(),
            half_bathrooms: "0".into(),
            minimum_stay: "1".into(),
            days_between_bookings: "0".into(),
            owner_id: owner.to_string(),
            is_active: "on".into(),
            booking_mode: BookingMode::Instant.as_str().into(),
            location_privacy: LocationPrivacy::Approximate.as_str().into(),
            slots: suggested_amenity_fields(),
            ..Default::default()
        }
    }
}

/// A validated listing.
#[derive(Debug, Clone)]
pub struct ValidListing {
    owner_id: Uuid,
    name: String,
    description: Option<String>,
    listing_structure: String,
    country: String,
    city: Option<String>,
    base_currency: String,
    price_per_night: Decimal,
    cleaning_fee: Option<Decimal>,
    damage_deposit: Option<Decimal>,
    extra_guest_fee: Option<Decimal>,
    weekly: Option<Decimal>,
    monthly: Option<Decimal>,
    commission_pct: Option<Decimal>,
    max_guests: i32,
    guests_included: i32,
    bedrooms: i32,
    beds: i32,
    full_bathrooms: i32,
    half_bathrooms: i32,
    square_meters: Option<i32>,
    minimum_stay: i32,
    days_between_bookings: i32,
    latitude: Option<f64>,
    longitude: Option<f64>,
    location_privacy: LocationPrivacy,
    is_active: bool,
    amenity_groups: Vec<AmenityGroup>,
    ribbon: Option<Vec<RibbonItem>>,
    suites: Vec<SuiteItem>,
    services: Option<StaffServices>,
    policies: Option<HousePolicies>,
    nearby: Vec<NearbyPlace>,
    guestbook: Vec<CuratedTestimonial>,
    booking_mode: BookingMode,
}

struct Parser {
    errors: Vec<String>,
}

impl Parser {
    fn text(&mut self, label: &str, v: &str) -> String {
        let v = v.trim();
        if v.is_empty() {
            self.errors.push(format!("{label} is required."));
        }
        v.to_string()
    }
    fn one_of(&mut self, label: &str, v: &str, allowed: &[&str]) -> String {
        if allowed.contains(&v) {
            v.to_string()
        } else {
            self.errors.push(format!("Choose a valid {label}."));
            allowed.first().copied().unwrap_or_default().to_string()
        }
    }
    /// A well-formed currency code; whether it is active is the API's call.
    fn currency(&mut self, v: &str) -> String {
        common::currency::CurrencyCode::try_from(v.trim())
            .map(String::from)
            .unwrap_or_else(|_| {
                self.errors.push("Choose a valid currency.".into());
                v.trim().to_string()
            })
    }
    fn int(&mut self, label: &str, v: &str, min: i32) -> i32 {
        match i32::from_str(v.trim()) {
            Ok(n) if n >= min => n,
            _ => {
                self.errors
                    .push(format!("{label} must be a whole number of at least {min}."));
                min
            }
        }
    }
    fn int_between(&mut self, label: &str, v: &str, min: i32, max: i32) -> i32 {
        match i32::from_str(v.trim()) {
            Ok(n) if (min..=max).contains(&n) => n,
            _ => {
                self.errors.push(format!(
                    "{label} must be a whole number from {min} to {max}."
                ));
                min
            }
        }
    }
    fn opt_int(&mut self, label: &str, v: &str) -> Option<i32> {
        let v = v.trim();
        if v.is_empty() {
            return None;
        }
        i32::from_str(v).ok().filter(|n| *n > 0).or_else(|| {
            self.errors
                .push(format!("{label} must be a positive whole number."));
            None
        })
    }
    fn money(&mut self, label: &str, v: &str) -> Option<Decimal> {
        let v = v.trim();
        if v.is_empty() {
            return None;
        }
        match Decimal::from_str(v) {
            Ok(d) if d >= Decimal::ZERO => Some(d.round_dp(2)),
            _ => {
                self.errors
                    .push(format!("{label} must be an amount of 0 or more."));
                None
            }
        }
    }
    fn percent(&mut self, label: &str, v: &str, max: Decimal) -> Option<Decimal> {
        let d = self.money(label, v)?;
        if d > max {
            self.errors.push(format!("{label} cannot exceed {max}."));
            return None;
        }
        Some(d)
    }
    fn coord(&mut self, label: &str, v: &str, limit: f64) -> Option<f64> {
        let v = v.trim();
        if v.is_empty() {
            return None;
        }
        // Coordinates, not money: f64 is the API's type.
        v.parse::<f64>()
            .ok()
            .filter(|x| x.abs() <= limit)
            .or_else(|| {
                self.errors
                    .push(format!("{label} must be between -{limit} and {limit}."));
                None
            })
    }
    /// Browsers submit textarea newlines as CRLF; store LF so the length
    /// check counts what the user typed.
    fn description(&mut self, v: &str) -> Option<String> {
        let v = v.replace("\r\n", "\n").trim().to_string();
        let len = v.chars().count();
        if len > MAX_DESCRIPTION_CHARS {
            self.errors.push(format!(
                "Description can be at most {MAX_DESCRIPTION_CHARS} characters (it is {len})."
            ));
        }
        Some(v).filter(|d| !d.is_empty())
    }
}

impl ListingFields {
    /// Validates the form. `owner` is used when the user may not choose.
    pub fn validate(
        &self,
        owner: Option<Uuid>,
        can_set_commission: bool,
    ) -> std::result::Result<ValidListing, Vec<String>> {
        let mut p = Parser { errors: Vec::new() };
        let owner_id = owner
            .or_else(|| Uuid::parse_str(self.owner_id.trim()).ok())
            .unwrap_or_else(|| {
                p.errors.push("Choose an owner.".into());
                Uuid::nil()
            });
        let name = p.text("Name", &self.name);
        let price = p
            .money("Nightly price", &self.price_per_night)
            .unwrap_or_else(|| {
                if self.price_per_night.trim().is_empty() {
                    p.errors.push("Nightly price is required.".into());
                }
                Decimal::ZERO
            });
        let valid = ValidListing {
            owner_id,
            name,
            description: p.description(&self.description),
            listing_structure: p.one_of("structure", &self.listing_structure, &STRUCTURES),
            country: p.text("Country", &self.country),
            city: Some(self.city.trim().to_string()).filter(|c| !c.is_empty()),
            base_currency: p.currency(&self.base_currency),
            price_per_night: price,
            cleaning_fee: p.money("Cleaning fee", &self.cleaning_fee),
            damage_deposit: p.money("Damage deposit", &self.damage_deposit),
            extra_guest_fee: p.money("Extra guest fee", &self.extra_guest_fee),
            weekly: p.percent(
                "Weekly discount",
                &self.weekly_discount_percentage,
                Decimal::ONE_HUNDRED,
            ),
            monthly: p.percent(
                "Monthly discount",
                &self.monthly_discount_percentage,
                Decimal::ONE_HUNDRED,
            ),
            commission_pct: if can_set_commission {
                p.percent("Commission", &self.commission_pct, Decimal::ONE)
            } else {
                None
            },
            max_guests: p.int("Maximum guests", &self.max_guests, 1),
            guests_included: p.int("Guests included", &self.guests_included, 1),
            bedrooms: p.int("Bedrooms", &self.bedrooms, 0),
            beds: p.int("Beds", &self.beds, 0),
            full_bathrooms: p.int("Full bathrooms", &self.full_bathrooms, 0),
            half_bathrooms: p.int("Half bathrooms", &self.half_bathrooms, 0),
            square_meters: p.opt_int("Floor area", &self.square_meters),
            minimum_stay: p.int("Minimum stay", &self.minimum_stay, 1),
            days_between_bookings: p.int_between(
                "Days between bookings",
                &self.days_between_bookings,
                0,
                365,
            ),
            latitude: p.coord("Latitude", &self.latitude, 90.0),
            longitude: p.coord("Longitude", &self.longitude, 180.0),
            location_privacy: parse_location_privacy(&mut p, &self.location_privacy),
            is_active: self.is_active == "on",
            amenity_groups: parse_amenity_groups(&self.slots).unwrap_or_else(|errors| {
                p.errors.extend(errors);
                Vec::new()
            }),
            ribbon: parse_ribbon(&self.slots).unwrap_or_else(|errors| {
                p.errors.extend(errors);
                None
            }),
            suites: parse_suites(&self.slots).unwrap_or_else(|errors| {
                p.errors.extend(errors);
                Vec::new()
            }),
            services: parse_services(&self.slots).unwrap_or_else(|errors| {
                p.errors.extend(errors);
                None
            }),
            policies: None,
            nearby: Vec::new(),
            guestbook: parse_guestbook(&self.slots).unwrap_or_else(|errors| {
                p.errors.extend(errors);
                Vec::new()
            }),
            booking_mode: parse_booking_mode(&mut p, &self.booking_mode),
        };
        let mut valid = valid;
        match parse_policies(&self.slots) {
            Ok((policies, nearby)) => {
                valid.policies = policies;
                valid.nearby = nearby;
            }
            Err(errors) => p.errors.extend(errors),
        }
        if valid.guests_included > valid.max_guests {
            p.errors
                .push("Guests included cannot exceed maximum guests.".into());
        }
        if p.errors.is_empty() {
            Ok(valid)
        } else {
            Err(p.errors)
        }
    }
}

/// The booking mode as posted; a blank value (the create form has no select)
/// means instant.
/// The location setting; blank means the default, approximate.
fn parse_location_privacy(p: &mut Parser, value: &str) -> LocationPrivacy {
    match value.trim() {
        "" | "approximate" => LocationPrivacy::Approximate,
        "exact" => LocationPrivacy::Exact,
        _ => {
            p.errors
                .push("Choose how the location is shown to guests.".to_string());
            LocationPrivacy::Approximate
        }
    }
}

fn parse_booking_mode(p: &mut Parser, value: &str) -> BookingMode {
    match value.trim() {
        "" => BookingMode::Instant,
        v => BookingMode::parse(v).unwrap_or_else(|| {
            p.errors.push("Choose a valid booking mode.".into());
            BookingMode::Instant
        }),
    }
}

impl ValidListing {
    /// `details` with the form's content sections written in: set when
    /// filled, removed when cleared, other keys untouched.
    fn content_details(
        &self,
        mut details: serde_json::Map<String, serde_json::Value>,
    ) -> serde_json::Map<String, serde_json::Value> {
        // Amenities now live in groups; a legacy flat list moves into them.
        details.remove(AMENITIES_KEY);
        if self.amenity_groups.is_empty() {
            details.remove(AMENITY_GROUPS_KEY);
        } else {
            details.insert(
                AMENITY_GROUPS_KEY.to_string(),
                serde_json::json!(self.amenity_groups),
            );
        }
        match &self.ribbon {
            Some(items) => {
                details.insert(RIBBON_KEY.to_string(), serde_json::json!(items));
            }
            None => {
                details.remove(RIBBON_KEY);
            }
        }
        if self.suites.is_empty() {
            details.remove(SUITES_KEY);
        } else {
            details.insert(SUITES_KEY.to_string(), serde_json::json!(self.suites));
        }
        match &self.services {
            Some(services) => {
                details.insert(STAFF_SERVICES_KEY.to_string(), serde_json::json!(services));
            }
            None => {
                details.remove(STAFF_SERVICES_KEY);
            }
        }
        match &self.policies {
            Some(policies) => {
                details.insert(POLICIES_KEY.to_string(), serde_json::json!(policies));
            }
            None => {
                details.remove(POLICIES_KEY);
            }
        }
        if self.nearby.is_empty() {
            details.remove(NEARBY_KEY);
        } else {
            details.insert(NEARBY_KEY.to_string(), serde_json::json!(self.nearby));
        }
        if self.guestbook.is_empty() {
            details.remove(TESTIMONIALS_KEY);
        } else {
            details.insert(
                TESTIMONIALS_KEY.to_string(),
                serde_json::json!(self.guestbook),
            );
        }
        details
    }

    /// The create request.
    pub fn into_new(self) -> NewListingRequest {
        let listing_details = Some(serde_json::Value::Object(
            self.content_details(serde_json::Map::new()),
        ));
        NewListingRequest {
            name: self.name,
            user_id: self.owner_id,
            description: self.description,
            listing_structure: self.listing_structure,
            country: self.country,
            price_per_night: Some(self.price_per_night),
            weekly_discount_percentage: self.weekly,
            monthly_discount_percentage: self.monthly,
            max_guests: self.max_guests,
            bedrooms: self.bedrooms,
            beds: self.beds,
            full_bathrooms: self.full_bathrooms,
            half_bathrooms: self.half_bathrooms,
            square_meters: self.square_meters,
            latitude: self.latitude,
            longitude: self.longitude,
            listing_details,
            city: self.city,
            base_currency: self.base_currency,
            minimum_stay: self.minimum_stay,
            days_between_bookings: self.days_between_bookings,
            commission_pct: self.commission_pct,
            cleaning_fee: self.cleaning_fee,
            damage_deposit: self.damage_deposit,
            guests_included: self.guests_included,
            extra_guest_fee: self.extra_guest_fee,
            is_active: self.is_active,
            location_privacy: Some(self.location_privacy),
        }
    }

    /// The update request (owner is not changed by an edit). `listing_api`
    /// replaces `listing_details` as a whole, so the amenities are merged
    /// into the listing's `current_details` to keep its other keys (Wi-Fi,
    /// address, check-in instructions).
    pub fn into_update(self, current_details: Option<&serde_json::Value>) -> UpdatedListingRequest {
        let details = self.content_details(
            current_details
                .and_then(|d| d.as_object())
                .cloned()
                .unwrap_or_default(),
        );
        UpdatedListingRequest {
            name: Some(self.name),
            description: self.description,
            listing_structure: Some(self.listing_structure),
            country: Some(self.country),
            base_currency: Some(self.base_currency),
            price_per_night: Some(self.price_per_night),
            weekly_discount_percentage: self.weekly,
            monthly_discount_percentage: self.monthly,
            max_guests: Some(self.max_guests),
            bedrooms: Some(self.bedrooms),
            beds: Some(self.beds),
            full_bathrooms: Some(self.full_bathrooms),
            half_bathrooms: Some(self.half_bathrooms),
            square_meters: self.square_meters,
            latitude: self.latitude,
            longitude: self.longitude,
            city: self.city,
            listing_details: Some(serde_json::Value::Object(details)),
            minimum_stay: Some(self.minimum_stay),
            days_between_bookings: Some(self.days_between_bookings),
            is_active: Some(self.is_active),
            commission_pct: self.commission_pct,
            cleaning_fee: self.cleaning_fee,
            damage_deposit: self.damage_deposit,
            guests_included: Some(self.guests_included),
            extra_guest_fee: self.extra_guest_fee,
            booking_mode: Some(self.booking_mode),
            location_privacy: Some(self.location_privacy),
        }
    }
}

/// One owner choice for admins: (user ID, label).
pub type OwnerOption = (String, String);

/// The studio form. `owners` is empty for hosts (they own what they make).
#[component]
pub async fn listing_form(
    cx: &Cx,
    /// Where the form posts.
    action: String,
    /// Submit button text.
    submit_label: String,
    /// Current values.
    fields: ListingFields,
    /// Owner choices (admins only).
    owners: Vec<OwnerOption>,
    /// Whether the commission field is shown (admins only).
    can_set_commission: bool,
    /// Validation or API errors to show.
    errors: Vec<String>,
    /// The listing's photos, for each suite's photo picker (none when new).
    #[default]
    suite_photos: Vec<SuitePhoto>,
) -> Result<impl View> {
    let label = "block text-xs font-medium text-muted-foreground";
    let input = "mt-1 block h-10 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";
    let f = fields;
    // (name, label, value, step, required)
    let num = |name: &'static str,
               title: &'static str,
               value: String,
               step: &'static str,
               required: bool| (name, title, value, step, required);
    // The create endpoint has no booking mode, so new listings start as
    // instant and the mode is set when editing.
    let editing = action != "/admin/listings";
    let money_fields = [
        num(
            "price_per_night",
            "Nightly price",
            f.price_per_night.clone(),
            "0.01",
            true,
        ),
        num(
            "cleaning_fee",
            "Cleaning fee",
            f.cleaning_fee.clone(),
            "0.01",
            false,
        ),
        num(
            "damage_deposit",
            "Damage deposit",
            f.damage_deposit.clone(),
            "0.01",
            false,
        ),
        num(
            "extra_guest_fee",
            "Extra guest fee (per night)",
            f.extra_guest_fee.clone(),
            "0.01",
            false,
        ),
        num(
            "weekly_discount_percentage",
            "Weekly discount (%)",
            f.weekly_discount_percentage.clone(),
            "0.01",
            false,
        ),
        num(
            "monthly_discount_percentage",
            "Monthly discount (%)",
            f.monthly_discount_percentage.clone(),
            "0.01",
            false,
        ),
    ];
    let count_fields = [
        num(
            "max_guests",
            "Maximum guests",
            f.max_guests.clone(),
            "1",
            true,
        ),
        num(
            "guests_included",
            "Guests included",
            f.guests_included.clone(),
            "1",
            true,
        ),
        num("bedrooms", "Bedrooms", f.bedrooms.clone(), "1", true),
        num("beds", "Beds", f.beds.clone(), "1", true),
        num(
            "full_bathrooms",
            "Full bathrooms",
            f.full_bathrooms.clone(),
            "1",
            true,
        ),
        num(
            "half_bathrooms",
            "Half bathrooms",
            f.half_bathrooms.clone(),
            "1",
            true,
        ),
        num(
            "square_meters",
            "Floor area (m²)",
            f.square_meters.clone(),
            "1",
            false,
        ),
        num(
            "minimum_stay",
            "Minimum stay (nights)",
            f.minimum_stay.clone(),
            "1",
            true,
        ),
        num(
            "days_between_bookings",
            "Days between bookings",
            f.days_between_bookings.clone(),
            "1",
            true,
        ),
    ];
    Ok(view! {
        <form id="listing-form" method="post" action=(action) class="space-y-8">
            if !errors.is_empty() {
                <div
                    id="listing-form-errors"
                    role="alert"
                    class="rounded-lg border border-destructive p-4 text-sm text-destructive"
                >
                    <ul class="list-disc pl-5">
                        for e in errors {
                            <li>(e)</li>
                        }
                    </ul>
                </div>
            }
            <p
                id="listing-required-note"
                class="flex items-center gap-1.5 text-xs text-muted-foreground"
            >
                <span
                    aria-hidden="true"
                    class="inline-block size-1.5 rounded-full bg-primary"
                ></span>
                "Required"
            </p>
            <fieldset
                class="grid gap-4 rounded-xl border border-border p-6 sm:grid-cols-2"
            >
                <legend class="px-1 text-sm font-semibold">"Basics"</legend>
                <label class=(label)>
                    "Name"
                    <span
                        aria-hidden="true"
                        data-required-dot=""
                        class="ml-1.5 inline-block size-1.5 rounded-full bg-primary align-middle"
                    ></span>
                    <input
                        id="listing-name"
                        name="name"
                        required=(true)
                        value=(f.name.clone())
                        class=(input)
                    />
                </label>
                <label class=(label)>
                    "Structure"
                    <span
                        aria-hidden="true"
                        data-required-dot=""
                        class="ml-1.5 inline-block size-1.5 rounded-full bg-primary align-middle"
                    ></span>
                    <select name="listing_structure" required=(true) class=(input)>
                        for s in STRUCTURES {
                            <option value=(s) selected=(f.listing_structure == s)>
                                (s)
                            </option>
                        }
                    </select>
                </label>
                <div class="sm:col-span-2">
                    <label class=(label)>
                        "Description"
                        <textarea
                            id="listing-description"
                            name="description"
                            rows="4"
                            maxlength=(MAX_DESCRIPTION_CHARS.to_string())
                            aria-describedby="listing-description-count"
                            class=(format!("{input} h-auto py-2"))
                        >
                            (f.description.clone())
                        </textarea>
                    </label>
                    <p
                        id="listing-description-count"
                        data-char-count="listing-description"
                        aria-live="polite"
                        class="mt-1 text-right text-xs text-muted-foreground"
                    >
                        (format!(
                            "{} / {}",
                            f.description.replace("\r\n", "\n").chars().count(),
                            MAX_DESCRIPTION_CHARS,
                        ))
                    </p>
                </div>
                <label class=(label)>
                    "Country"
                    <span
                        aria-hidden="true"
                        data-required-dot=""
                        class="ml-1.5 inline-block size-1.5 rounded-full bg-primary align-middle"
                    ></span>
                    <input
                        name="country"
                        required=(true)
                        value=(f.country.clone())
                        class=(input)
                    />
                </label>
                <label class=(label)>
                    "City"
                    <input name="city" value=(f.city.clone()) class=(input) />
                </label>
                <label class=(label)>
                    "Latitude"
                    <input
                        name="latitude"
                        inputmode="decimal"
                        value=(f.latitude.clone())
                        class=(input)
                    />
                </label>
                <label class=(label)>
                    "Longitude"
                    <input
                        name="longitude"
                        inputmode="decimal"
                        value=(f.longitude.clone())
                        class=(input)
                    />
                </label>
                <fieldset
                    id="location-privacy"
                    aria-describedby="location-privacy-help"
                    class="md:col-span-2"
                >
                    <legend class=(label)>"Location shown to guests before booking"</legend>
                    <div class="mt-2 flex flex-wrap gap-4 text-sm">
                        for (value, title) in [
                            ("approximate", "Approximate area (500 m)"),
                            ("exact", "Exact location"),
                        ] {
                            <label class="inline-flex items-center gap-2">
                                <input
                                    type="radio"
                                    name="location_privacy"
                                    value=(value)
                                    checked=(f.location_privacy == value
                                        || (value == "approximate" && f.location_privacy.is_empty()))
                                />
                                (title)
                            </label>
                        }
                    </div>
                    <p id="location-privacy-help" class="mt-1 text-xs text-muted-foreground">
                        "Switching back to Approximate can't hide a location guests have already seen."
                    </p>
                </fieldset>
                if !owners.is_empty() {
                    <label class=(label)>
                        "Owner"
                        <span
                            aria-hidden="true"
                            data-required-dot=""
                            class="ml-1.5 inline-block size-1.5 rounded-full bg-primary align-middle"
                        ></span>
                        <select name="owner_id" required=(true) class=(input)>
                            for (id, owner_label) in owners {
                                <option value=(id.clone()) selected=(f.owner_id == id)>
                                    (owner_label)
                                </option>
                            }
                        </select>
                    </label>
                }
                if editing {
                    <label class=(label)>
                        "Booking mode"
                        <select
                            id="listing-booking-mode"
                            name="booking_mode"
                            aria-describedby="listing-booking-mode-help"
                            class=(input)
                        >
                            for mode in BookingMode::ALL {
                                <option
                                    value=(mode.as_str())
                                    selected=(f.booking_mode == mode.as_str())
                                >
                                    (mode.label())
                                </option>
                            }
                        </select>
                        <span
                            id="listing-booking-mode-help"
                            class="mt-1 block text-xs text-muted-foreground"
                        >
                            "Request to book lets you approve each stay first. Open requests and offers carry on if you change this."
                        </span>
                    </label>
                }
                <label class="flex items-center gap-2 text-sm">
                    <input
                        type="checkbox"
                        name="is_active"
                        checked=(f.is_active == "on")
                        class="size-4"
                    />
                    "Bookable (active)"
                </label>
            </fieldset>
            <fieldset
                class="grid gap-4 rounded-xl border border-border p-6 sm:grid-cols-3"
            >
                <legend class="px-1 text-sm font-semibold">"Pricing & fees"</legend>
                <label class=(label)>
                    "Base currency"
                    <span
                        aria-hidden="true"
                        data-required-dot=""
                        class="ml-1.5 inline-block size-1.5 rounded-full bg-primary align-middle"
                    ></span>
                    <select name="base_currency" required=(true) class=(input)>
                        currency_option_list(current: f.base_currency.clone())
                    </select>
                </label>
                for (name, title, value, step, required) in money_fields {
                    <label class=(label)>
                        (title)
                        if required {
                            <span
                                aria-hidden="true"
                                data-required-dot=""
                                class="ml-1.5 inline-block size-1.5 rounded-full bg-primary align-middle"
                            ></span>
                        }
                        <input
                            name=(name)
                            type="number"
                            min="0"
                            step=(step)
                            required=(required)
                            value=(value)
                            class=(input)
                        />
                    </label>
                }
                if can_set_commission {
                    <label class=(label)>
                        "Commission (0–1)"
                        <input
                            name="commission_pct"
                            type="number"
                            min="0"
                            max="1"
                            step="0.0001"
                            value=(f.commission_pct.clone())
                            class=(input)
                        />
                    </label>
                }
            </fieldset>
            <fieldset
                class="grid gap-4 rounded-xl border border-border p-6 sm:grid-cols-3"
            >
                <legend class="px-1 text-sm font-semibold">
                    "Capacity & stay rules"
                </legend>
                for (name, title, value, step, required) in count_fields {
                    <label class=(label)>
                        (title)
                        if required {
                            <span
                                aria-hidden="true"
                                data-required-dot=""
                                class="ml-1.5 inline-block size-1.5 rounded-full bg-primary align-middle"
                            ></span>
                        }
                        <input
                            name=(name)
                            type="number"
                            min="0"
                            step=(step)
                            required=(required)
                            value=(value)
                            class=(input)
                        />
                    </label>
                }
            </fieldset>
            ribbon_fieldset(
                fields: f.slots.clone(),
                label_class: label,
                input_class: input
            )
            amenity_fieldset(
                fields: f.slots.clone(),
                label_class: label,
                input_class: input
            )
            suite_fieldset(
                fields: f.slots.clone(),
                photos: suite_photos,
                label_class: label,
                input_class: input
            )
            services_fieldset(
                fields: f.slots.clone(),
                label_class: label,
                input_class: input
            )
            policies_fieldset(
                fields: f.slots.clone(),
                label_class: label,
                input_class: input
            )
            guestbook_fieldset(
                fields: f.slots.clone(),
                label_class: label,
                input_class: input
            )
            <button
                id="btn-save-listing"
                type="submit"
                class="h-10 rounded-lg bg-primary px-6 text-sm font-medium text-primary-foreground"
            >
                (submit_label)
            </button>
        </form>
    })
}

/// The reasons `listing_api` gave for rejecting a create or update, read
/// from the JSON body `common::app_client` puts in its error. `None` when
/// the error carries no such body (e.g. the API was unreachable).
pub fn api_rejection_messages(err: &dyn std::fmt::Display) -> Option<Vec<String>> {
    let text = err.to_string();
    let body: serde_json::Value = serde_json::from_str(&text[text.find('{')?..]).ok()?;
    let obj = body.as_object()?;
    // `{"error": "..."}` / `{"code": ..., "message": "..."}`
    if let Some(msg) = ["message", "error"]
        .iter()
        .find_map(|k| obj.get(*k).and_then(|v| v.as_str()))
    {
        return Some(vec![msg.to_string()]);
    }
    // validator's `{"field": [{"code": ..., "message": ...}]}`
    let messages: Vec<String> = obj
        .iter()
        .flat_map(|(field, errs)| {
            errs.as_array().into_iter().flatten().map(move |e| {
                match e.get("message").and_then(|m| m.as_str()) {
                    Some(m) => m.to_string(),
                    None => format!(
                        "{field}: {}",
                        e.get("code").and_then(|c| c.as_str()).unwrap_or("invalid")
                    ),
                }
            })
        })
        .collect();
    Some(messages).filter(|m| !m.is_empty())
}

#[cfg(test)]
mod listing_booking_mode {
    use super::*;

    fn fields(mode: &str) -> ListingFields {
        ListingFields {
            name: "Villa".into(),
            price_per_night: "200".into(),
            booking_mode: mode.into(),
            ..ListingFields::blank(Uuid::nil())
        }
    }

    #[test]
    fn the_chosen_mode_is_sent_on_update() {
        for mode in BookingMode::ALL {
            let update = fields(mode.as_str())
                .validate(Some(Uuid::nil()), false)
                .map(|v| v.into_update(None));
            assert_eq!(update.ok().and_then(|u| u.booking_mode), Some(mode));
        }
    }

    #[test]
    fn a_blank_mode_is_instant_and_an_unknown_one_is_refused() {
        let blank = fields("")
            .validate(Some(Uuid::nil()), false)
            .map(|v| v.into_update(None));
        assert_eq!(
            blank.ok().and_then(|u| u.booking_mode),
            Some(BookingMode::Instant)
        );
        let errors = fields("sometimes")
            .validate(Some(Uuid::nil()), false)
            .err()
            .unwrap_or_default();
        assert!(
            errors.iter().any(|e| e.contains("booking mode")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_listing_fills_the_form_with_its_mode() {
        assert_eq!(ListingFields::blank(Uuid::nil()).booking_mode, "instant");
    }
}
