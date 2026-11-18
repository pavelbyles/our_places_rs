//! Tests for `/admin/staff-users` authorization, host scoping, and `/admin/users` `Staff` isolation.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use common::http_client::{AuthenticatedClient, TokenProvider};
use common::models::{ListingResponse, SessionResponse};
use common::turnover::{StaffListResponse, StaffListingAssignment, StaffMemberSummary};
use reqwest::StatusCode;
use tokio::net::TcpListener;
use topcoat::cookie::CookieLayer;
use topcoat::router::{Router, layout};
use topcoat::session::{SessionTokenStore, cookie::CookieTokenStore};
use topcoat::view::{Child, View, view};
use uuid::Uuid;
use web_client::{TopcoatApiClient, jwt::ServiceJwt};
use web_ui_admin::pages::staff_users::{
    invite_staff_submit, remove_staff_submit, resend_staff_invite_submit, staff_users_page,
    update_staff_listings_submit,
};
use web_ui_admin::pages::users::list::users_page;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct FixedIdToken;

#[async_trait::async_trait]
impl TokenProvider for FixedIdToken {
    async fn get_token(&self, _audience: &str) -> anyhow::Result<String> {
        Ok("service-id-token".to_string())
    }
}

#[layout("/")]
async fn test_layout(cx: &topcoat::context::Cx, slot: Child<'_>) -> topcoat::Result<impl View> {
    let _ = cx;
    Ok(view! { <div>(slot)</div> })
}

async fn serve(api: &MockServer) -> Result<String, Box<dyn std::error::Error>> {
    let service_jwt = ServiceJwt::from_secret("test-jwt-secret-for-staff-ui")?;
    let sessions = SessionTokenStore::new(CookieTokenStore::new().name("op_admin_session"));
    let client = TopcoatApiClient::new(
        AuthenticatedClient::with_token_provider(Arc::new(FixedIdToken)),
        api.uri(),
        api.uri(),
        api.uri(),
    );
    let router = Router::builder()
        .layout(test_layout)
        .page(staff_users_page)
        .page(invite_staff_submit)
        .page(update_staff_listings_submit)
        .page(resend_staff_invite_submit)
        .page(remove_staff_submit)
        .page(users_page)
        .page(web_ui_admin::pages::auth::login_submit)
        .layer(CookieLayer::new())
        .with(sessions)
        .with(client)
        .with(service_jwt)
        .build();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = topcoat::serve(listener, router).await;
    });
    Ok(format!("http://{addr}"))
}

fn no_redirect_client() -> Result<reqwest::Client, Box<dyn std::error::Error>> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

fn cookie_pair_from(res: &reqwest::Response) -> Option<String> {
    res.headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .find_map(|v| v.to_str().ok()?.split(';').next().map(str::to_string))
}

fn sample_listing(owner_id: Uuid, id: Uuid, name: &str, now: DateTime<Utc>) -> ListingResponse {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "user_id": owner_id,
        "name": name,
        "listing_structure": "Villa",
        "country": "Jamaica",
        "is_active": true,
        "added_at": now,
        "max_guests": 4,
        "bedrooms": 2,
        "beds": 2,
        "full_bathrooms": 2,
        "half_bathrooms": 0,
        "base_currency": "USD",
        "slug": format!("v-{}", id.simple()),
        "minimum_stay": 1,
        "days_between_bookings": 0
    }))
    .unwrap_or_else(|e| panic!("listing fixture: {e}"))
}

#[tokio::test]
async fn staff_gets_403_and_host_sees_own_staff_and_listings()
-> Result<(), Box<dyn std::error::Error>> {
    let api = MockServer::start().await;
    let base = serve(&api).await?;
    let client = no_redirect_client()?;
    let now = Utc::now();

    let staff_id = Uuid::now_v7();
    let host_id = Uuid::now_v7();
    let other_host_id = Uuid::now_v7();
    let owned_villa = Uuid::now_v7();
    let other_villa = Uuid::now_v7();

    let staff_session = SessionResponse {
        token_hash: "tok-staff".into(),
        user_id: staff_id,
        email: "crew@ourplaces.example".into(),
        name: "Crew Member".into(),
        role: "staff".into(),
        namespace: "admin".into(),
        created_at: now,
        last_accessed_at: now,
        expires_at: now.timestamp() + 3600,
    };
    let host_session = SessionResponse {
        token_hash: "tok-host".into(),
        user_id: host_id,
        email: "host@ourplaces.example".into(),
        name: "Host Owner".into(),
        role: "host".into(),
        namespace: "admin".into(),
        created_at: now,
        last_accessed_at: now,
        expires_at: now.timestamp() + 3600,
    };

    // 1. Staff session receives 403 on both `/admin/staff-users` and `/admin/users`.
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            r"^/api/v1/sessions/[0-9a-f]+$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(staff_session.clone()))
        .mount(&api)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/users/login"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(common::models::UserResponse {
                id: staff_id,
                email: "crew@ourplaces.example".into(),
                first_name: "Crew".into(),
                last_name: "Member".into(),
                phone_number: None,
                is_active: true,
                is_verified: true,
                verification_code: None,
                created_at: now,
                updated_at: now,
                attributes: serde_json::json!({}),
                roles: vec!["staff".into()],
                default_currency: "USD".into(),
                deleted_at: None,
                slug: "crew-member".into(),
            }),
        )
        .mount(&api)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions"))
        .respond_with(ResponseTemplate::new(201).set_body_json(staff_session))
        .mount(&api)
        .await;

    let staff_login = client
        .post(format!("{base}/login"))
        .form(&[("email", "crew@ourplaces.example"), ("password", "pw")])
        .send()
        .await?;
    let staff_cookie = cookie_pair_from(&staff_login).ok_or("staff cookie")?;

    let staff_on_staff_users = client
        .get(format!("{base}/admin/staff-users"))
        .header(reqwest::header::COOKIE, &staff_cookie)
        .send()
        .await?;
    assert_eq!(staff_on_staff_users.status(), StatusCode::FORBIDDEN);

    let staff_on_admin_users = client
        .get(format!("{base}/admin/users"))
        .header(reqwest::header::COOKIE, &staff_cookie)
        .send()
        .await?;
    assert_eq!(staff_on_admin_users.status(), StatusCode::FORBIDDEN);

    // 2. Host session sees their own staff roster and ONLY owned listings.
    let api_host = MockServer::start().await;
    let base_host = serve(&api_host).await?;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            r"^/api/v1/sessions/[0-9a-f]+$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(host_session.clone()))
        .mount(&api_host)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/users/login"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(common::models::UserResponse {
                id: host_id,
                email: "host@ourplaces.example".into(),
                first_name: "Host".into(),
                last_name: "Owner".into(),
                phone_number: None,
                is_active: true,
                is_verified: true,
                verification_code: None,
                created_at: now,
                updated_at: now,
                attributes: serde_json::json!({}),
                roles: vec!["host".into()],
                default_currency: "USD".into(),
                deleted_at: None,
                slug: "host-owner".into(),
            }),
        )
        .mount(&api_host)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions"))
        .respond_with(ResponseTemplate::new(201).set_body_json(host_session))
        .mount(&api_host)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![
            sample_listing(host_id, owned_villa, "Owned Coral House", now),
            sample_listing(other_host_id, other_villa, "Secret Stranger Villa", now),
        ]))
        .mount(&api_host)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/staff-users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(StaffListResponse {
            staff: vec![StaffMemberSummary {
                user_id: staff_id,
                email: "crew@ourplaces.example".into(),
                first_name: "Dana".into(),
                last_name: "Crew".into(),
                is_active: true,
                is_verified: false,
                password_pending: true,
                listings: vec![StaffListingAssignment {
                    listing_id: owned_villa,
                    listing_name: "Owned Coral House".into(),
                    assigned_at: now,
                }],
                invited_at: now,
            }],
            total: 1,
            page: 1,
            per_page: 20,
        }))
        .mount(&api_host)
        .await;

    let host_login = client
        .post(format!("{base_host}/login"))
        .form(&[("email", "host@ourplaces.example"), ("password", "pw")])
        .send()
        .await?;
    let host_cookie = cookie_pair_from(&host_login).ok_or("host cookie")?;

    let host_page = client
        .get(format!("{base_host}/admin/staff-users"))
        .header(reqwest::header::COOKIE, &host_cookie)
        .send()
        .await?;
    assert_eq!(host_page.status(), StatusCode::OK);
    let body = host_page.text().await?;
    assert!(body.contains("Dana Crew"), "{body}");
    assert!(body.contains("Owned Coral House"), "{body}");
    assert!(!body.contains("Secret Stranger Villa"), "{body}");

    Ok(())
}
