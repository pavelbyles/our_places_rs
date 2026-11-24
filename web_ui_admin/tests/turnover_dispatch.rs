//! Integration tests for `/admin/turnovers` board, `/admin/turnovers/{id}` detail, and HTMX comments.

use std::sync::Arc;

use chrono::Utc;
use common::http_client::{AuthenticatedClient, TokenProvider};
use common::models::{ListingResponse, SessionResponse};
use common::turnover::{
    TurnoverCommentResponse, TurnoverDetailResponse, TurnoverListResponse, TurnoverResponse,
    TurnoverSourceKind, TurnoverStatus, TurnoverStatusHistoryEntry,
};
use reqwest::StatusCode;
use tokio::net::TcpListener;
use topcoat::cookie::CookieLayer;
use topcoat::router::{Router, layout};
use topcoat::session::{SessionTokenStore, cookie::CookieTokenStore};
use topcoat::view::{Child, View, view};
use uuid::Uuid;
use web_client::{TopcoatApiClient, jwt::ServiceJwt};
use web_ui_admin::pages::turnovers::{
    create_turnover_action, post_turnover_comment_action, transition_turnover_action,
    turnover_detail_page, turnovers_page,
};
use wiremock::matchers::{method, path, path_regex};
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
    let service_jwt = ServiceJwt::from_secret("test-jwt-secret-for-turnovers-ui")?;
    let sessions = SessionTokenStore::new(CookieTokenStore::new().name("op_admin_session"));
    let client = TopcoatApiClient::new(
        AuthenticatedClient::with_token_provider(Arc::new(FixedIdToken)),
        api.uri(),
        api.uri(),
        api.uri(),
    );
    let router = Router::builder()
        .layout(test_layout)
        .page(turnovers_page)
        .page(create_turnover_action)
        .page(transition_turnover_action)
        .page(turnover_detail_page)
        .page(post_turnover_comment_action)
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

async fn login_cookie(
    client: &reqwest::Client,
    api: &MockServer,
    base: &str,
    user_id: Uuid,
    email: &str,
    role: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let now = Utc::now();
    let session = SessionResponse {
        token_hash: format!("tok-{role}-{user_id}"),
        user_id,
        email: email.to_string(),
        name: format!("{role} User"),
        role: role.to_string(),
        namespace: "admin".to_string(),
        created_at: now,
        last_accessed_at: now,
        expires_at: now.timestamp() + 3600,
    };
    api.reset().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v1/sessions/[0-9a-f]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session.clone()))
        .mount(api)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/users/login"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(common::models::UserResponse {
                id: user_id,
                email: email.to_string(),
                first_name: role.to_string(),
                last_name: "User".to_string(),
                phone_number: None,
                is_active: true,
                is_verified: true,
                verification_code: None,
                created_at: now,
                updated_at: now,
                attributes: serde_json::json!({}),
                roles: vec![role.to_string()],
                slug: format!("{role}-user"),
                default_currency: "USD".to_string(),
                deleted_at: None,
            }),
        )
        .mount(api)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions"))
        .respond_with(ResponseTemplate::new(201).set_body_json(session.clone()))
        .mount(api)
        .await;
    let res = client
        .post(format!("{base}/login"))
        .form(&[("email", email), ("password", "secret")])
        .send()
        .await?;
    Ok(cookie_pair_from(&res).ok_or("missing session cookie")?)
}

fn sample_turnover(id: Uuid, listing_id: Uuid, status: TurnoverStatus) -> TurnoverResponse {
    TurnoverResponse {
        id,
        listing_id,
        listing_name: "Sea Breeze Villa".to_string(),
        listing_city: Some("Ocho Rios".to_string()),
        checkin_policy_text: Some("3:00 PM".to_string()),
        checkout_policy_text: Some("11:00 AM".to_string()),
        source_kind: TurnoverSourceKind::Booking,
        checkout_date: chrono::NaiveDate::from_ymd_opt(2026, 11, 18).unwrap_or_default(),
        next_checkin_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 11, 20).unwrap_or_default()),
        status,
        title: None,
        assigned_staff_names: vec!["Staff User".to_string()],
        comment_count: 1,
        updated_at: Utc::now(),
    }
}

#[tokio::test]
async fn board_tailors_transitions_by_role_and_renders_upstream_alert()
-> Result<(), Box<dyn std::error::Error>> {
    let api = MockServer::start().await;
    let base = serve(&api).await?;
    let client = no_redirect_client()?;

    let staff_id = Uuid::now_v7();
    let host_id = Uuid::now_v7();
    let listing_id = Uuid::now_v7();
    let turnover_id = Uuid::now_v7();

    // 1. Staff views board with an InspectedAndReady booking turnover.
    let staff_cookie =
        login_cookie(&client, &api, &base, staff_id, "staff@example.com", "staff").await?;
    Mock::given(method("GET"))
        .and(path("/api/v1/turnovers"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(TurnoverListResponse {
                turnovers: vec![sample_turnover(
                    turnover_id,
                    listing_id,
                    TurnoverStatus::InspectedAndReady,
                )],
                total: 1,
                page: 1,
                per_page: 20,
            }),
        )
        .mount(&api)
        .await;

    let staff_html = client
        .get(format!("{base}/admin/turnovers"))
        .header(reqwest::header::COOKIE, &staff_cookie)
        .send()
        .await?
        .text()
        .await?;
    assert!(staff_html.contains("Sea Breeze Villa"));
    assert!(staff_html.contains("data-status-target=\"maintenance_required\""));
    assert!(staff_html.contains("data-status-target=\"needs_cleaning\""));
    assert!(!staff_html.contains("data-status-target=\"inspected_and_ready\""));
    assert!(!staff_html.contains("/admin/bookings"));

    // 2. Host views board when turnover is CleaningCompleted -> Inspected & Ready action is rendered.
    let host_cookie =
        login_cookie(&client, &api, &base, host_id, "host@example.com", "host").await?;
    let host_listing: ListingResponse = serde_json::from_value(serde_json::json!({
        "id": listing_id,
        "user_id": host_id,
        "name": "Sea Breeze Villa",
        "listing_structure": "Villa",
        "country": "Jamaica",
        "is_active": true,
        "added_at": Utc::now(),
        "max_guests": 4,
        "bedrooms": 2,
        "beds": 2,
        "full_bathrooms": 2,
        "half_bathrooms": 0,
        "base_currency": "USD",
        "slug": "sea-breeze",
        "minimum_stay": 1,
        "days_between_bookings": 0
    }))?;
    Mock::given(method("GET"))
        .and(path("/api/v1/listings/my-listings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![host_listing]))
        .mount(&api)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/turnovers"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(TurnoverListResponse {
                turnovers: vec![sample_turnover(
                    turnover_id,
                    listing_id,
                    TurnoverStatus::CleaningCompleted,
                )],
                total: 1,
                page: 1,
                per_page: 20,
            }),
        )
        .mount(&api)
        .await;

    let host_html = client
        .get(format!("{base}/admin/turnovers"))
        .header(reqwest::header::COOKIE, &host_cookie)
        .send()
        .await?
        .text()
        .await?;
    assert!(host_html.contains("data-status-target=\"inspected_and_ready\""));

    // 3. Upstream API failure renders visible role="alert" banner instead of silent empty state.
    api.reset().await;
    let staff_cookie_2 =
        login_cookie(&client, &api, &base, staff_id, "staff@example.com", "staff").await?;
    Mock::given(method("GET"))
        .and(path("/api/v1/turnovers"))
        .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
            "code": "SERVICE_UNAVAILABLE",
            "message": "Database temporarily busy"
        })))
        .mount(&api)
        .await;

    let err_resp = client
        .get(format!("{base}/admin/turnovers"))
        .header(reqwest::header::COOKIE, &staff_cookie_2)
        .send()
        .await?;
    assert_eq!(err_resp.status(), StatusCode::OK);
    let err_html = err_resp.text().await?;
    assert!(err_html.contains("data-testid=\"turnover-load-error\""));
    assert!(err_html.contains("Database temporarily busy"));

    Ok(())
}

#[tokio::test]
async fn detail_htmx_comments_and_conflict_handling_and_authz_work()
-> Result<(), Box<dyn std::error::Error>> {
    let api = MockServer::start().await;
    let base = serve(&api).await?;
    let client = no_redirect_client()?;

    let staff_id = Uuid::now_v7();
    let listing_id = Uuid::now_v7();
    let turnover_id = Uuid::now_v7();
    let hidden_id = Uuid::now_v7();
    let now = Utc::now();

    let staff_cookie =
        login_cookie(&client, &api, &base, staff_id, "staff@example.com", "staff").await?;

    let initial_comment = TurnoverCommentResponse {
        id: Uuid::now_v7(),
        turnover_id,
        author_user_id: Some(Uuid::now_v7()),
        author_name: "Host Owner".to_string(),
        author_role: "host".to_string(),
        body: "Please check patio cushions.".to_string(),
        created_at: now,
    };
    let detail_payload = TurnoverDetailResponse {
        turnover: sample_turnover(turnover_id, listing_id, TurnoverStatus::NeedsCleaning),
        status_history: vec![TurnoverStatusHistoryEntry {
            id: Uuid::now_v7(),
            turnover_id,
            from_status: None,
            to_status: TurnoverStatus::NeedsCleaning,
            actor_user_id: None,
            actor_name: None,
            actor_role: "system".to_string(),
            entry_point: "booking_confirm".to_string(),
            reason: None,
            created_at: now,
        }],
        comments: vec![initial_comment],
    };

    Mock::given(method("GET"))
        .and(path(format!("/api/v1/turnovers/{turnover_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(detail_payload.clone()))
        .mount(&api)
        .await;

    let detail_resp = client
        .get(format!("{base}/admin/turnovers/{turnover_id}"))
        .header(reqwest::header::COOKIE, &staff_cookie)
        .send()
        .await?;
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail_html = detail_resp.text().await?;
    assert!(detail_html.contains("Sea Breeze Villa"));
    assert!(detail_html.contains("Please check patio cushions."));
    assert!(detail_html.contains("id=\"turnover-comments-list\""));
    assert!(detail_html.contains("data-reset-on-success=\"\""));

    // HTMX comment post returns only the new chat_bubble fragment without full document wrapper.
    let new_comment_id = Uuid::now_v7();
    let posted_comment = TurnoverCommentResponse {
        id: new_comment_id,
        turnover_id,
        author_user_id: Some(staff_id),
        author_name: "Staff User".to_string(),
        author_role: "staff".to_string(),
        body: "Patio cushions secured and dry.".to_string(),
        created_at: Utc::now(),
    };
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/turnovers/{turnover_id}/comments")))
        .respond_with(ResponseTemplate::new(201).set_body_json(posted_comment))
        .mount(&api)
        .await;

    let htmx_resp = client
        .post(format!("{base}/admin/turnovers/{turnover_id}/comments"))
        .header(reqwest::header::COOKIE, &staff_cookie)
        .header("HX-Request", "true")
        .form(&[("body", "Patio cushions secured and dry.")])
        .send()
        .await?;
    assert_eq!(htmx_resp.status(), StatusCode::OK);
    let fragment = htmx_resp.text().await?;
    assert!(fragment.contains(&format!("turnover-comment-{new_comment_id}")));
    assert!(fragment.contains("Patio cushions secured and dry."));
    assert!(!fragment.contains("<!DOCTYPE"));

    // Unauthorized turnover ID returns 404/403 without leaking listing or comments.
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/turnovers/{hidden_id}")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&api)
        .await;
    let hidden_resp = client
        .get(format!("{base}/admin/turnovers/{hidden_id}"))
        .header(reqwest::header::COOKIE, &staff_cookie)
        .send()
        .await?;
    assert_eq!(hidden_resp.status(), StatusCode::NOT_FOUND);
    let hidden_body = hidden_resp.text().await?;
    assert!(!hidden_body.contains("Sea Breeze Villa"));
    assert!(!hidden_body.contains("Patio cushions"));

    // Concurrent status conflict (409) on HTMX transition refreshes card with inline alert.
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/turnovers/{turnover_id}/status")))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "code": "TURNOVER_STATUS_CONFLICT",
            "message": "Concurrent update"
        })))
        .mount(&api)
        .await;

    let conflict_resp = client
        .post(format!("{base}/admin/turnovers/{turnover_id}/status"))
        .header(reqwest::header::COOKIE, &staff_cookie)
        .header("HX-Request", "true")
        .form(&[
            ("to_status", "cleaning_in_progress"),
            ("expected_status", "needs_cleaning"),
        ])
        .send()
        .await?;
    assert_eq!(conflict_resp.status(), StatusCode::OK);
    let conflict_html = conflict_resp.text().await?;
    assert!(conflict_html.contains("Status changed concurrently elsewhere"));

    Ok(())
}
