use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ListingResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub listing_structure: String, // Simplified from enum for common compatibility if needed, or move enum here
    pub country: String,
    pub price_per_night: Option<Decimal>,
    pub is_active: bool,
    pub added_at: DateTime<Utc>,
}
