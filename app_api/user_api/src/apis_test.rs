use actix_web::{App, test, web};
use api_core::health::health_check;

#[actix_web::test]
async fn test_health_check() {
    // Create the app with just the health check route
    let app = test::init_service(
        App::new().route("/api/v1/users/health_check", web::get().to(health_check)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/users/health_check")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}
