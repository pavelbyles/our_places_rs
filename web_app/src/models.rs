use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ListingResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub price_per_night: Option<f64>,
}
