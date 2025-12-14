pub mod listing {
    tonic::include_proto!("listing"); // The string must match `package listing;` in your proto
}

#[cfg(test)]
mod tests {
    use super::listing::{ListingRequest, ListingResponse};

    #[test]
    fn can_create_listing_request_and_response() {
        let req = ListingRequest {
            id: "123".to_string(),
        };
        assert_eq!(req.id, "123");

        let resp = ListingResponse {
            name: "Test Listing".to_string(),
            description: "A sample description".to_string(),
        };
        assert_eq!(resp.name, "Test Listing");
        assert_eq!(resp.description, "A sample description");
    }
}
