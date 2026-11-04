use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

// Struct to hold pagination query parameters.
/// Pagination query parameters.
#[derive(Deserialize, ToSchema, IntoParams, Debug)]
pub struct Pagination {
    /// Page number, starting at 1.
    pub page: Option<u32>,
    /// Results per page.
    pub per_page: Option<u32>,
}
