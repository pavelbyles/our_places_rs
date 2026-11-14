//! Spec 103 §1.9: the edit form takes its location from the manager-only
//! endpoint, never from a listing response, and posts the privacy setting.

use std::sync::Arc;

use common::http_client::{AuthenticatedClient, TokenProvider};
use common::location::{ListingLocationResponse, LocationPrivacy};
use common::models::ListingResponse;
use topcoat::context::Cx;
use topcoat::view::{ViewExt, view};
use uuid::Uuid;
use web_client::client::TopcoatApiClient;
use web_ui_admin::pages::listings::edit::edit_prefill;
use web_ui_admin::pages::listings::form::{ListingFields, listing_form};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct FixedIdToken;

#[async_trait::async_trait]
impl TokenProvider for FixedIdToken {
    async fn get_token(&self, _audience: &str) -> anyhow::Result<String> {
        Ok("service-id-token".to_string())
    }
}

fn client_for(server: &MockServer) -> TopcoatApiClient {
    TopcoatApiClient::new(
        AuthenticatedClient::with_token_provider(Arc::new(FixedIdToken)),
        server.uri(),
        server.uri(),
        server.uri(),
    )
}

/// A listing as a public path returns it: carrying the approximate point.
fn public_listing(id: Uuid) -> ListingResponse {
    let value = serde_json::json!({
        "id": id, "user_id": Uuid::now_v7(), "name": "Sea Grape Villa",
        "listing_structure": "Villa", "country": "Jamaica", "is_active": true,
        "added_at": "2026-11-01T00:00:00Z", "max_guests": 4, "bedrooms": 2, "beds": 2,
        "full_bathrooms": 2, "half_bathrooms": 0, "base_currency": "USD",
        "slug": "sea-grape", "minimum_stay": 1, "days_between_bookings": 0,
        "latitude": 18.41000, "longitude": -77.10000, "location_precision": "approximate"
    });
    match serde_json::from_value(value) {
        Ok(listing) => listing,
        Err(e) => panic!("fixture: {e}"),
    }
}

async fn html(v: impl topcoat::view::View, cx: &Cx) -> String {
    match v.first().await {
        Ok(handle) => handle.render(cx),
        Err(e) => panic!("render failed: {e}"),
    }
}

#[tokio::test]
async fn the_edit_form_uses_the_stored_point_not_the_public_one() {
    let server = MockServer::start().await;
    let id = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/listings/{id}/location")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "latitude": 18.40741, "longitude": -77.10313, "location_privacy": "approximate"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let location = match edit_prefill(&client_for(&server), id, "user-token").await {
        Ok(location) => location,
        Err(e) => panic!("{e}"),
    };
    let fields = ListingFields::from_listing(&public_listing(id)).with_location(&location);
    assert_eq!(fields.latitude, "18.40741");
    assert_eq!(fields.longitude, "-77.10313");
    assert_eq!(fields.location_privacy, "approximate");

    // Without the endpoint the form holds no point at all, never the public one.
    let bare = ListingFields::from_listing(&public_listing(id));
    assert_eq!((bare.latitude.as_str(), bare.longitude.as_str()), ("", ""));
}

#[tokio::test]
async fn a_failed_location_load_is_an_error_the_page_turns_into_503() {
    let server = MockServer::start().await;
    let id = Uuid::now_v7();
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/listings/{id}/location")))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    assert!(
        edit_prefill(&client_for(&server), id, "user-token")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_privacy_radio_defaults_to_approximate_and_shows_the_stored_value() {
    let cx = &Cx::default();
    let render = |fields: ListingFields| async move {
        html(
            view! {
                listing_form(
                    action: "/admin/listings".to_string(),
                    submit_label: "Create".to_string(),
                    fields: fields,
                    owners: Vec::new(),
                    can_set_commission: false,
                    errors: Vec::new()
                )
            },
            cx,
        )
        .await
    };
    let checked = |out: &str, value: &str| {
        out.contains(&format!(r#"value="{value}" checked"#))
            || out.contains(&format!(r#"checked value="{value}""#))
            || out
                .split("<input")
                .any(|tag| tag.contains(&format!(r#"value="{value}""#)) && tag.contains("checked"))
    };

    let blank = render(ListingFields::blank(Uuid::now_v7())).await;
    assert!(
        blank.contains("Location shown to guests before booking"),
        "{blank}"
    );
    assert!(checked(&blank, "approximate"), "{blank}");
    assert!(!checked(&blank, "exact"), "{blank}");
    assert!(blank.contains("can't hide a location guests have already seen"));

    let exact = ListingFields::blank(Uuid::now_v7()).with_location(&ListingLocationResponse {
        latitude: Some(18.40741),
        longitude: Some(-77.10313),
        location_privacy: LocationPrivacy::Exact,
    });
    let out = render(exact).await;
    assert!(checked(&out, "exact"), "{out}");
    assert!(!checked(&out, "approximate"), "{out}");
}

#[test]
fn the_posted_setting_reaches_the_request() {
    let base = || ListingFields {
        name: "Sea Grape Villa".into(),
        price_per_night: "450".into(),
        ..ListingFields::blank(Uuid::now_v7())
    };
    let create = base().validate(None, false).map(|v| v.into_new());
    assert_eq!(
        create.ok().and_then(|r| r.location_privacy),
        Some(LocationPrivacy::Approximate)
    );
    let exact = ListingFields {
        location_privacy: "exact".into(),
        ..base()
    };
    let update = exact.validate(None, false).map(|v| v.into_update(None));
    assert_eq!(
        update.ok().and_then(|r| r.location_privacy),
        Some(LocationPrivacy::Exact)
    );
    let bad = ListingFields {
        location_privacy: "nearby".into(),
        ..base()
    };
    let errors = bad.validate(None, false).err().unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("how the location is shown")),
        "{errors:?}"
    );
}
