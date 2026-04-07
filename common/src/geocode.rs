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

    let res = client.get(&url).send().await?;

    if let Ok(data) = res.json::<NominatimResponse>().await {
        if let Some(address) = data.address {
            let locality = address
                .city
                .or(address.town)
                .or(address.village)
                .or(address.suburb)
                .or(address.neighbourhood)
                .or(address.county);
            return Ok(locality);
        }
    }

    Ok(None)
}
