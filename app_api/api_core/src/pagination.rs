use serde::{Deserialize};
use utoipa::{IntoParams, ToSchema};

// Struct to hold pagination query parameters.
#[derive(Deserialize, ToSchema, IntoParams, Debug)]
pub struct Pagination {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}
