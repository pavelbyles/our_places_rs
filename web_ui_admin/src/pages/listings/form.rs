//! The listing studio form shared by create, clone and edit.

use std::str::FromStr;

use common::models::{ListingResponse, NewListingRequest, UpdatedListingRequest};
use rust_decimal::Decimal;
use serde::Deserialize;
use topcoat::{
    Result,
    view::{View, component, view},
};
use uuid::Uuid;

/// Listing structures known to `listing_api`.
pub const STRUCTURES: [&str; 5] = ["Villa", "House", "Apartment", "Townhouse", "Studio"];
/// Base currencies a listing can be priced in.
pub const CURRENCIES: [&str; 5] = ["USD", "JMD", "EUR", "GBP", "CAD"];
/// Most amenities a listing can list.
pub const MAX_AMENITIES: usize = 50;
/// Longest amenity, in characters.
pub const MAX_AMENITY_CHARS: usize = 60;
/// Longest description `listing_api` accepts, in characters.
pub const MAX_DESCRIPTION_CHARS: usize = 2000;
/// Key of the amenity list inside `listing_details` (the guest listing page
/// reads the same key).
const AMENITIES_KEY: &str = "amenities";

/// The amenities stored in a listing's `listing_details`.
pub fn amenities_of(details: Option<&serde_json::Value>) -> Vec<String> {
    details
        .and_then(|d| d.get(AMENITIES_KEY))
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

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
    /// Amenities, one per line.
    pub amenities: String,
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
            latitude: l.latitude.map(|v| v.to_string()).unwrap_or_default(),
            longitude: l.longitude.map(|v| v.to_string()).unwrap_or_default(),
            owner_id: l.user_id.to_string(),
            is_active: if l.is_active {
                "on".into()
            } else {
                String::new()
            },
            amenities: amenities_of(l.listing_details.as_ref()).join("\n"),
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
    is_active: bool,
    amenities: Vec<String>,
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
    /// One entry per non-blank line, trimmed, without case-insensitive
    /// duplicates (the first spelling wins).
    fn amenities(&mut self, v: &str) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let list: Vec<String> = v
            .lines()
            .map(str::trim)
            .filter(|a| !a.is_empty() && seen.insert(a.to_lowercase()))
            .map(str::to_string)
            .collect();
        if list.len() > MAX_AMENITIES {
            self.errors
                .push(format!("List at most {MAX_AMENITIES} amenities."));
        }
        if let Some(long) = list.iter().find(|a| a.chars().count() > MAX_AMENITY_CHARS) {
            self.errors.push(format!(
                "Amenities can be at most {MAX_AMENITY_CHARS} characters (\"{long}\" is longer)."
            ));
        }
        list
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
            base_currency: p.one_of("currency", &self.base_currency, &CURRENCIES),
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
            days_between_bookings: p.int("Days between bookings", &self.days_between_bookings, 0),
            latitude: p.coord("Latitude", &self.latitude, 90.0),
            longitude: p.coord("Longitude", &self.longitude, 180.0),
            is_active: self.is_active == "on",
            amenities: p.amenities(&self.amenities),
        };
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

impl ValidListing {
    /// The create request.
    pub fn into_new(self) -> NewListingRequest {
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
            listing_details: Some(serde_json::json!({
                "is_active": self.is_active,
                AMENITIES_KEY: self.amenities,
            })),
            city: self.city,
            base_currency: self.base_currency,
            minimum_stay: self.minimum_stay,
            days_between_bookings: self.days_between_bookings,
            commission_pct: self.commission_pct,
            cleaning_fee: self.cleaning_fee,
            damage_deposit: self.damage_deposit,
            guests_included: self.guests_included,
            extra_guest_fee: self.extra_guest_fee,
        }
    }

    /// The update request (owner is not changed by an edit). `listing_api`
    /// replaces `listing_details` as a whole, so the amenities are merged
    /// into the listing's `current_details` to keep its other keys (Wi-Fi,
    /// address, check-in instructions).
    pub fn into_update(self, current_details: Option<&serde_json::Value>) -> UpdatedListingRequest {
        let mut details = current_details
            .and_then(|d| d.as_object())
            .cloned()
            .unwrap_or_default();
        details.insert(AMENITIES_KEY.to_string(), serde_json::json!(self.amenities));
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
        }
    }
}

/// One owner choice for admins: (user ID, label).
pub type OwnerOption = (String, String);

/// The studio form. `owners` is empty for hosts (they own what they make).
#[component]
pub async fn listing_form(
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
) -> Result<impl View> {
    let label = "block text-xs font-medium text-muted-foreground";
    let input = "mt-1 block h-10 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";
    let f = fields;
    let num = |name: &'static str, title: &'static str, value: String, step: &'static str| {
        (name, title, value, step)
    };
    let money_fields = [
        num(
            "price_per_night",
            "Nightly price",
            f.price_per_night.clone(),
            "0.01",
        ),
        num(
            "cleaning_fee",
            "Cleaning fee",
            f.cleaning_fee.clone(),
            "0.01",
        ),
        num(
            "damage_deposit",
            "Damage deposit",
            f.damage_deposit.clone(),
            "0.01",
        ),
        num(
            "extra_guest_fee",
            "Extra guest fee (per night)",
            f.extra_guest_fee.clone(),
            "0.01",
        ),
        num(
            "weekly_discount_percentage",
            "Weekly discount (%)",
            f.weekly_discount_percentage.clone(),
            "0.01",
        ),
        num(
            "monthly_discount_percentage",
            "Monthly discount (%)",
            f.monthly_discount_percentage.clone(),
            "0.01",
        ),
    ];
    let count_fields = [
        num("max_guests", "Maximum guests", f.max_guests.clone(), "1"),
        num(
            "guests_included",
            "Guests included",
            f.guests_included.clone(),
            "1",
        ),
        num("bedrooms", "Bedrooms", f.bedrooms.clone(), "1"),
        num("beds", "Beds", f.beds.clone(), "1"),
        num(
            "full_bathrooms",
            "Full bathrooms",
            f.full_bathrooms.clone(),
            "1",
        ),
        num(
            "half_bathrooms",
            "Half bathrooms",
            f.half_bathrooms.clone(),
            "1",
        ),
        num(
            "square_meters",
            "Floor area (m²)",
            f.square_meters.clone(),
            "1",
        ),
        num(
            "minimum_stay",
            "Minimum stay (nights)",
            f.minimum_stay.clone(),
            "1",
        ),
        num(
            "days_between_bookings",
            "Days between bookings",
            f.days_between_bookings.clone(),
            "1",
        ),
    ];
    Ok(view! {
        <form id="listing-form" method="post" action=(action) class="space-y-8">
            if !errors.is_empty() {
                <div id="listing-form-errors" role="alert" class="rounded-lg border border-destructive p-4 text-sm text-destructive">
                    <ul class="list-disc pl-5">
                        for e in errors {
                            <li>(e)</li>
                        }
                    </ul>
                </div>
            }
            <fieldset class="grid gap-4 rounded-xl border border-border p-6 sm:grid-cols-2">
                <legend class="px-1 text-sm font-semibold">"Basics"</legend>
                <label class=(label)>"Name"<input id="listing-name" name="name" required=(true) value=(f.name.clone()) class=(input) /></label>
                <label class=(label)>"Structure"
                    <select name="listing_structure" class=(input)>
                        for s in STRUCTURES {
                            <option value=(s) selected=(f.listing_structure == s)>(s)</option>
                        }
                    </select>
                </label>
                <label class=(format!("{label} sm:col-span-2"))>"Description"<textarea name="description" rows="4" maxlength=(MAX_DESCRIPTION_CHARS.to_string()) class=(format!("{input} h-auto py-2"))>(f.description.clone())</textarea></label>
                <label class=(label)>"Country"<input name="country" required=(true) value=(f.country.clone()) class=(input) /></label>
                <label class=(label)>"City"<input name="city" value=(f.city.clone()) class=(input) /></label>
                <label class=(label)>"Latitude"<input name="latitude" inputmode="decimal" value=(f.latitude.clone()) class=(input) /></label>
                <label class=(label)>"Longitude"<input name="longitude" inputmode="decimal" value=(f.longitude.clone()) class=(input) /></label>
                if !owners.is_empty() {
                    <label class=(label)>"Owner"
                        <select name="owner_id" class=(input)>
                            for (id, owner_label) in owners {
                                <option value=(id.clone()) selected=(f.owner_id == id)>(owner_label)</option>
                            }
                        </select>
                    </label>
                }
                <label class="flex items-center gap-2 text-sm"><input type="checkbox" name="is_active" checked=(f.is_active == "on") class="size-4" />"Bookable (active)"</label>
            </fieldset>
            <fieldset class="grid gap-4 rounded-xl border border-border p-6 sm:grid-cols-3">
                <legend class="px-1 text-sm font-semibold">"Pricing & fees"</legend>
                <label class=(label)>"Base currency"
                    <select name="base_currency" class=(input)>
                        for c in CURRENCIES {
                            <option value=(c) selected=(f.base_currency == c)>(c)</option>
                        }
                    </select>
                </label>
                for (name, title, value, step) in money_fields {
                    <label class=(label)>(title)<input name=(name) type="number" min="0" step=(step) value=(value) class=(input) /></label>
                }
                if can_set_commission {
                    <label class=(label)>"Commission (0–1)"<input name="commission_pct" type="number" min="0" max="1" step="0.0001" value=(f.commission_pct.clone()) class=(input) /></label>
                }
            </fieldset>
            <fieldset class="grid gap-4 rounded-xl border border-border p-6 sm:grid-cols-3">
                <legend class="px-1 text-sm font-semibold">"Capacity & stay rules"</legend>
                for (name, title, value, step) in count_fields {
                    <label class=(label)>(title)<input name=(name) type="number" min="0" step=(step) value=(value) class=(input) /></label>
                }
            </fieldset>
            <fieldset class="grid gap-2 rounded-xl border border-border p-6">
                <legend class="px-1 text-sm font-semibold">"Amenities"</legend>
                <label for="listing-amenities" class=(label)>"One per line, e.g. Private pool, Wi-Fi, Air conditioning. Guests see them under “What this place offers”."</label>
                <textarea id="listing-amenities" name="amenities" rows="6" class=(format!("{input} h-auto py-2"))>(f.amenities.clone())</textarea>
            </fieldset>
            <button id="btn-save-listing" type="submit" class="h-10 rounded-lg bg-primary px-6 text-sm font-medium text-primary-foreground">(submit_label)</button>
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
