//! A lazily loaded map (spec 103 §1.11). Renders nothing when maps are off
//! (no valid `MAP_TILES_BASE_URL`, see [`crate::map_config`]); otherwise an
//! empty box of fixed height that `ui.js` fills once it nears the viewport.

use common::location::{LocationPrecision, in_bounds};
use common::models::ListingResponse;
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{Attributes, View, class, component, view},
};

use crate::assets::{MAP_JS, MAPLIBRE_CSS, MAPLIBRE_JS, MAPLIBRE_WORKER_JS, PMTILES_JS};
use crate::map_config::map_config;

/// What a map shows; `map.js` sets each one up differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    /// `/listings`: a badge per result card.
    Search,
    /// A listing page: one point, a circle when approximate.
    Listing,
    /// A confirmed booking: the exact point.
    Booking,
    /// The admin pin picker.
    Picker,
}

impl MapKind {
    /// The `data-map` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Listing => "listing",
            Self::Booking => "booking",
            Self::Picker => "picker",
        }
    }
}

/// One point and how precise it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapPoint {
    /// Latitude.
    pub lat: f64,
    /// Longitude.
    pub lng: f64,
    /// Exact, or a public approximate point.
    pub precision: LocationPrecision,
}

impl MapPoint {
    /// A listing's public point, or `None` when it has no coordinates or they
    /// lie outside the mapped area. A point of unknown precision is treated as
    /// public, never as exact.
    pub fn of_listing(listing: &ListingResponse) -> Option<Self> {
        listing
            .latitude
            .zip(listing.longitude)
            .filter(|&(lat, lng)| in_bounds(lat, lng))
            .map(|(lat, lng)| Self {
                lat,
                lng,
                precision: listing
                    .location_precision
                    .unwrap_or(LocationPrecision::Approximate),
            })
    }

    /// The `data-precision` value.
    pub fn precision_str(&self) -> &'static str {
        precision_attr(self.precision)
    }
}

fn precision_attr(precision: LocationPrecision) -> &'static str {
    match precision {
        LocationPrecision::Exact => "exact",
        LocationPrecision::Approximate => "approximate",
    }
}

/// The map container. Its height comes from `class` (default `h-56`) and is
/// reserved before anything loads, so loading shifts nothing.
#[component]
pub async fn map_container(
    cx: &Cx,
    /// What the map shows.
    kind: MapKind,
    /// The point, for single-point maps.
    #[default]
    point: Option<MapPoint>,
    /// Accessible name of the map region.
    #[default]
    label: Option<String>,
    /// Extra HTML attributes for the root element; `class` is merged with the defaults.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let config = map_config(cx).cloned();
    let label = label.unwrap_or_else(|| "Map".to_string());
    Ok(view! {
        if let Some(config) = config {
            <div
                data-map=(kind.as_str())
                data-map-js=(MAPLIBRE_JS)
                data-map-worker=(MAPLIBRE_WORKER_JS)
                data-map-css=(MAPLIBRE_CSS)
                data-pmtiles-js=(PMTILES_JS)
                data-map-init=(MAP_JS)
                data-tiles-base=(config.base_url().to_string())
                data-lat=(point.map(|p| p.lat.to_string()))
                data-lng=(point.map(|p| p.lng.to_string()))
                data-precision=(point.map(|p| precision_attr(p.precision)))
                role="region"
                aria-label=(label)
                class=(class!(
                    "relative h-56 w-full overflow-hidden rounded-xl border border-border bg-muted",
                    attrs.remove("class"),
                ))
                (attrs)
            >
                // Sized, not positioned: MapLibre's stylesheet makes this
                // element `position: relative`, which would undo `inset-0`.
                <div data-map-canvas="" class="h-full w-full"></div>
                <p
                    data-map-fallback=""
                    hidden=(true)
                    class="absolute inset-0 grid place-items-center text-sm text-muted-foreground"
                >
                    "Map unavailable"
                </p>
                <noscript>
                    <p class="absolute inset-0 grid place-items-center text-sm text-muted-foreground">
                        "Map needs JavaScript"
                    </p>
                </noscript>
            </div>
        }
    })
}

#[cfg(test)]
#[path = "map_tests.rs"]
mod map_component_tests;
