use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct NominatimResponse {
    address: Option<NominatimAddress>,
}

#[derive(Deserialize)]
struct NominatimAddress {
    city: Option<String>,
    town: Option<String>,
    village: Option<String>,
    suburb: Option<String>,
    neighbourhood: Option<String>,
    county: Option<String>,
}

/// Fetches the city/town/village name from OpenStreetMap
pub async fn reverse_geocode(lat: f64, lon: f64) -> Result<Option<String>, reqwest::Error> {
    let client = Client::builder().user_agent("OurPlaces").build()?;

    let url = format!(
        "https://nominatim.openstreetmap.org/reverse?format=jsonv2&lat={}&lon={}",
        lat, lon
    );

    let locality = client
        .get(&url)
        .send()
        .await?
        .json::<NominatimResponse>()
        .await
        .ok()
        .and_then(|data| data.address)
        .and_then(|addr| {
            addr.city
                .or(addr.town)
                .or(addr.village)
                .or(addr.suburb)
                .or(addr.neighbourhood)
                .or(addr.county)
        });

    Ok(locality)
}
