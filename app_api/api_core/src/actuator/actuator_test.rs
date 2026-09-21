use super::*;
use actix_web::test;
use actix_web::{App, http::StatusCode, web};
use serde_json::Value;
use uuid::Uuid;

#[actix_web::test]
async fn test_liveness_probe_unconditional_200() {
    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::get()
        .uri("/health/liveness")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: ProbeResponse = test::read_body_json(resp).await;
    assert_eq!(body.status, HealthStatus::Up);
}

#[actix_web::test]
async fn test_startup_probe_without_pool_returns_503() {
    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::get().uri("/health/startup").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: ProbeResponse = test::read_body_json(resp).await;
    assert_eq!(body.status, HealthStatus::Down);
}

#[actix_web::test]
async fn test_readiness_probe_without_pool_returns_503() {
    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::get()
        .uri("/health/readiness")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: ProbeResponse = test::read_body_json(resp).await;
    assert_eq!(body.status, HealthStatus::Down);
}

#[actix_web::test]
async fn test_composite_health_without_pool_returns_503() {
    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: HealthResponse = test::read_body_json(resp).await;
    assert_eq!(body.status, HealthStatus::Down);
    assert!(body.components.contains_key("db"));
    assert_eq!(body.components["db"].status, HealthStatus::Down);
}

#[actix_web::test]
async fn test_legacy_health_check_returns_404() {
    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::get().uri("/health_check").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn test_info_endpoint_returns_json() {
    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::get().uri("/info").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: AppInfoResponse = test::read_body_json(resp).await;
    assert_eq!(body.app.name, "api_core");
    assert!(!body.app.version.is_empty());
    assert!(!body.build.timestamp.is_empty());
}

#[actix_web::test]
async fn test_metrics_endpoint_render() {
    let prometheus_handle = init_metrics_recorder();
    let app = test::init_service(
        App::new()
            .wrap(ActuatorMetricsMiddleware)
            .app_data(web::Data::new(prometheus_handle))
            .configure(configure_actuator),
    )
    .await;

    // Trigger a request to generate counter and duration metrics
    let health_req = test::TestRequest::get()
        .uri("/health/liveness")
        .to_request();
    let _ = test::call_service(&app, health_req).await;

    let req = test::TestRequest::get().uri("/metrics").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.contains("text/plain"));

    let body = test::read_body(resp).await;
    let body_str = String::from_utf8_lossy(&body);

    assert!(body_str.contains("http_requests_total"));
}

#[actix_web::test]
async fn test_get_loggers_returns_valid_config() {
    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::get().uri("/loggers").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: LoggersResponse = test::read_body_json(resp).await;
    assert!(body.levels.contains(&"INFO".to_string()));
    assert!(body.levels.contains(&"DEBUG".to_string()));
    assert!(body.loggers.contains_key("ROOT"));
}

#[actix_web::test]
async fn test_post_loggers_unauthorized_without_credentials() {
    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::post()
        .uri("/loggers/ROOT")
        .set_json(&UpdateLoggerRequest {
            configured_level: "DEBUG".to_string(),
        })
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_post_loggers_authorized_with_secret_token() {
    unsafe {
        std::env::set_var("ACTUATOR_SECRET", "super-secret-token");
    }

    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::post()
        .uri("/loggers/ROOT")
        .insert_header(("x-actuator-token", "super-secret-token"))
        .set_json(&UpdateLoggerRequest {
            configured_level: "DEBUG".to_string(),
        })
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: UpdateLoggerResponse = test::read_body_json(resp).await;
    assert_eq!(body.configured_level, "DEBUG");
}

#[actix_web::test]
async fn test_post_loggers_authorized_with_admin_jwt() {
    unsafe {
        std::env::set_var("JWT_SECRET", "test-jwt-secret");
    }

    // Generate JWT token
    let test_user_id = Uuid::new_v4();
    let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize;

    #[derive(serde::Serialize)]
    struct TestClaims {
        sub: Uuid,
        exp: usize,
        role: String,
    }

    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &TestClaims {
            sub: test_user_id,
            exp,
            role: "admin".to_string(),
        },
        &jsonwebtoken::EncodingKey::from_secret(b"test-jwt-secret"),
    )
    .unwrap();

    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::post()
        .uri("/loggers/ROOT")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&UpdateLoggerRequest {
            configured_level: "WARN".to_string(),
        })
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: UpdateLoggerResponse = test::read_body_json(resp).await;
    assert_eq!(body.configured_level, "WARN");
}

#[actix_web::test]
async fn test_post_loggers_invalid_level_rejected() {
    unsafe {
        std::env::set_var("ACTUATOR_SECRET", "super-secret-token");
    }

    let app = test::init_service(App::new().configure(configure_actuator)).await;

    let req = test::TestRequest::post()
        .uri("/loggers/ROOT")
        .insert_header(("x-actuator-token", "super-secret-token"))
        .set_json(serde_json::json!({
            "configuredLevel": "INVALID_LEVEL"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "INVALID_LOG_LEVEL");
}

#[actix_web::test]
async fn test_health_probes_with_pool_integration() {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".to_string());

    if let Ok(pool) = sqlx::PgPool::connect(&db_url).await {
        let pool_data = web::Data::new(pool);
        let app = test::init_service(
            App::new()
                .app_data(pool_data.clone())
                .configure(configure_actuator),
        )
        .await;

        // 1. Startup probe
        let req = test::TestRequest::get().uri("/health/startup").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: ProbeResponse = test::read_body_json(resp).await;
        assert_eq!(body.status, HealthStatus::Up);

        // 2. Readiness probe
        let req = test::TestRequest::get()
            .uri("/health/readiness")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: ProbeResponse = test::read_body_json(resp).await;
        assert_eq!(body.status, HealthStatus::Up);

        // 3. Composite health
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: HealthResponse = test::read_body_json(resp).await;
        assert_eq!(body.status, HealthStatus::Up);
        assert!(body.components.contains_key("db"));
        let db_component = &body.components["db"];
        assert_eq!(db_component.status, HealthStatus::Up);
        assert!(db_component.details.is_some());
        let details = db_component.details.as_ref().unwrap();
        assert_eq!(details.database.as_deref(), Some("PostgreSQL"));
        assert!(details.latency_ms.is_some());
    }
}

#[actix_web::test]
async fn test_cloud_trace_context_extraction() {
    let app = test::init_service(
        App::new()
            .wrap(tracing_actix_web::TracingLogger::<
                crate::tracing_utils::CustomRootSpanBuilder,
            >::new())
            .configure(configure_actuator),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/health/liveness")
        .insert_header((
            "X-Cloud-Trace-Context",
            "105445aa7843bc8bf206b12000100000/1;o=1",
        ))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
