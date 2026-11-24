//! Integration tests for `/`, `/login`, `/verify`, `/forgot-password`, and `/reset-password`
//! verifying `Staff` `303` routing and expired-token recovery prompts.

use std::sync::Arc;

use common::http_client::{AuthenticatedClient, TokenProvider};
use reqwest::StatusCode;
use tokio::net::TcpListener;
use topcoat::cookie::CookieLayer;
use topcoat::router::{Router, layout};
use topcoat::session::{SessionTokenStore, cookie::CookieTokenStore};
use topcoat::view::{Child, View, view};
use uuid::Uuid;
use web_client::{TopcoatApiClient, jwt::ServiceJwt};
use web_ui_admin::pages::{
    auth::{login_page, login_submit},
    dashboard::root_page,
    password_reset::{
        forgot_password_page, forgot_password_submit, reset_password_page, reset_password_submit,
    },
    register::verify_submit,
};
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

#[layout("/")]
async fn test_layout(cx: &topcoat::context::Cx, slot: Child<'_>) -> topcoat::Result<impl View> {
    let _ = cx;
    Ok(view! { <div>(slot)</div> })
}

async fn serve(api: &MockServer) -> Result<String, Box<dyn std::error::Error>> {
    let service_jwt = ServiceJwt::from_secret("test-jwt-secret-for-admin-tests")?;
    let sessions = SessionTokenStore::new(CookieTokenStore::new().name("op_admin_session"));
    let router = Router::builder()
        .layout(test_layout)
        .page(root_page)
        .page(login_page)
        .page(login_submit)
        .page(verify_submit)
        .page(forgot_password_page)
        .page(forgot_password_submit)
        .page(reset_password_page)
        .page(reset_password_submit)
        .layer(CookieLayer::new())
        .with(sessions)
        .with(client_for(api))
        .with(service_jwt)
        .build();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = topcoat::serve(listener, router).await;
    });
    Ok(format!("http://{addr}"))
}

fn http_no_redirect() -> Result<reqwest::Client, Box<dyn std::error::Error>> {
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

fn staff_user(user_id: Uuid) -> common::models::UserResponse {
    let now = chrono::Utc::now();
    common::models::UserResponse {
        id: user_id,
        email: "ken@ourplaces.example".into(),
        first_name: "Ken".into(),
        last_name: "Cleaner".into(),
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
        slug: "ken-cleaner".into(),
    }
}

fn staff_session(user_id: Uuid) -> common::models::SessionResponse {
    let now = chrono::Utc::now();
    common::models::SessionResponse {
        token_hash: "mock-token-hash".into(),
        user_id,
        email: "ken@ourplaces.example".into(),
        name: "Ken Cleaner".into(),
        role: "staff".into(),
        namespace: "admin".into(),
        created_at: now,
        last_accessed_at: now,
        expires_at: now.timestamp() + 3600,
    }
}

#[tokio::test]
async fn staff_redirects_on_root_login_and_verify() -> Result<(), Box<dyn std::error::Error>> {
    let api = MockServer::start().await;
    let user_id = Uuid::now_v7();
    let base = serve(&api).await?;
    let client = http_no_redirect()?;

    // Signed-out `/` uses 303 See Other (never 308 Permanent Redirect).
    let root_unauth = client.get(format!("{base}/")).send().await?;
    assert_eq!(root_unauth.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        root_unauth
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/admin")
    );

    Mock::given(method("POST"))
        .and(path("/api/v1/users/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(staff_user(user_id)))
        .mount(&api)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/users/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(staff_user(user_id)))
        .mount(&api)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions"))
        .respond_with(ResponseTemplate::new(201).set_body_json(staff_session(user_id)))
        .mount(&api)
        .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            r"^/api/v1/sessions/[0-9a-f]+$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(staff_session(user_id)))
        .mount(&api)
        .await;

    // Login with forbidden `/admin/bookings` redirect sends Staff to `/admin/turnovers`.
    let login_res = client
        .post(format!("{base}/login"))
        .form(&[
            ("email", "ken@ourplaces.example"),
            ("password", "secret-pass"),
            ("redirect", "/admin/bookings"),
        ])
        .send()
        .await?;
    assert_eq!(login_res.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        login_res
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/admin/turnovers")
    );
    let session_cookie = cookie_pair_from(&login_res).ok_or("missing session Set-Cookie header")?;

    // Signed-in Staff hitting `/` gets 303 to `/admin/turnovers`.
    let root_staff = client
        .get(format!("{base}/"))
        .header(reqwest::header::COOKIE, &session_cookie)
        .send()
        .await?;
    assert_eq!(root_staff.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        root_staff
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/admin/turnovers")
    );

    // Signed-in Staff hitting `/login?redirect=/admin/turnovers?window=today` preserves dispatch redirect.
    let login_dispatch_redir = client
        .get(format!(
            "{base}/login?redirect=%2Fadmin%2Fturnovers%3Fwindow%3Dtoday"
        ))
        .header(reqwest::header::COOKIE, &session_cookie)
        .send()
        .await?;
    assert_eq!(login_dispatch_redir.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        login_dispatch_redir
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/admin/turnovers?window=today")
    );

    // Signed-in Staff hitting `/login?redirect=/admin/bookings` drops non-dispatch redirect.
    let login_forbidden_redir = client
        .get(format!("{base}/login?redirect=%2Fadmin%2Fbookings"))
        .header(reqwest::header::COOKIE, &session_cookie)
        .send()
        .await?;
    assert_eq!(login_forbidden_redir.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        login_forbidden_redir
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/admin/turnovers")
    );

    // Fresh client verifying OTP as Staff also redirects 303 to `/admin/turnovers`.
    let fresh = http_no_redirect()?;
    let verify_res = fresh
        .post(format!("{base}/verify"))
        .form(&[("email", "ken@ourplaces.example"), ("code", "123456")])
        .send()
        .await?;
    assert_eq!(verify_res.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        verify_res
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/admin/turnovers")
    );

    Ok(())
}

#[tokio::test]
async fn forgot_and_reset_password_pages_handle_valid_and_expired_tokens()
-> Result<(), Box<dyn std::error::Error>> {
    let api = MockServer::start().await;
    let base = serve(&api).await?;
    let client = http_no_redirect()?;

    Mock::given(method("POST"))
        .and(path("/api/v1/users/password-reset/request"))
        .respond_with(
            ResponseTemplate::new(202).set_body_json(serde_json::json!({ "status": "accepted" })),
        )
        .expect(1)
        .mount(&api)
        .await;

    let forgot_get = client.get(format!("{base}/forgot-password")).send().await?;
    assert_eq!(forgot_get.status(), StatusCode::OK);
    let forgot_html = forgot_get.text().await?;
    assert!(forgot_html.contains(r#"action="/forgot-password""#));

    let forgot_post = client
        .post(format!("{base}/forgot-password"))
        .form(&[("email", "ken@ourplaces.example")])
        .send()
        .await?;
    assert_eq!(forgot_post.status(), StatusCode::OK);
    let sent_html = forgot_post.text().await?;
    assert!(sent_html.contains(r#"id="forgot-password-sent""#));

    // Missing token shows inline recovery prompt.
    let missing_token = client.get(format!("{base}/reset-password")).send().await?;
    assert_eq!(missing_token.status(), StatusCode::OK);
    let missing_html = missing_token.text().await?;
    assert!(missing_html.contains(r#"id="reset-password-expired""#));
    assert!(missing_html.contains(r#"href="/forgot-password""#));

    // Valid token renders the form.
    let form_page = client
        .get(format!("{base}/reset-password?token=valid-token-1"))
        .send()
        .await?;
    assert_eq!(form_page.status(), StatusCode::OK);
    assert!(form_page.text().await?.contains("valid-token-1"));

    // Expired/used token returning 401 renders inline recovery prompt.
    Mock::given(method("POST"))
        .and(path("/api/v1/users/password-reset/confirm"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "code": "UNAUTHORIZED",
            "message": "Invalid or expired password reset token"
        })))
        .expect(1)
        .mount(&api)
        .await;

    let expired_submit = client
        .post(format!("{base}/reset-password"))
        .form(&[
            ("token", "used-token"),
            ("new_password", "new-secret-123"),
            ("confirm_password", "new-secret-123"),
        ])
        .send()
        .await?;
    assert_eq!(expired_submit.status(), StatusCode::OK);
    let expired_html = expired_submit.text().await?;
    assert!(expired_html.contains(r#"id="reset-password-expired""#));
    assert!(expired_html.contains(r#"href="/forgot-password""#));

    // Fresh server for success path -> 303 to `/login?reset=1`.
    let api_ok = MockServer::start().await;
    let base_ok = serve(&api_ok).await?;
    Mock::given(method("POST"))
        .and(path("/api/v1/users/password-reset/confirm"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "status": "password_updated" })),
        )
        .expect(1)
        .mount(&api_ok)
        .await;

    let ok_submit = client
        .post(format!("{base_ok}/reset-password"))
        .form(&[
            ("token", "good-token"),
            ("new_password", "new-secret-123"),
            ("confirm_password", "new-secret-123"),
        ])
        .send()
        .await?;
    assert_eq!(ok_submit.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        ok_submit
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/login?reset=1")
    );

    Ok(())
}
