use sqlx::PgPool;
mod common;

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}

#[tokio::test]
async fn health_check_works() {
    let app = common::spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health/health_check", &app.address))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());
}

#[tokio::test]
async fn health_check_response() {
    let app = common::spawn_app().await;
    let client = reqwest::Client::new();

    let body = client
        .get(&format!("{}/health/health_check", &app.address))
        .send()
        .await
        .expect("Failed to execute request")
        .text()
        .await
        .expect("Couldn't get text from response body");

    let ping_resp: our_places_app_api_listing_rs::apis::health::PingResponse =
        serde_json::from_str(&body).unwrap();
    assert_eq!(ping_resp.status.to_string(), "alive")
}

#[tokio::test]
async fn post_request_with_data_returns_200_test() {
    let app = common::spawn_app().await;
    let client = reqwest::Client::new();

    let body = "name=pavel%20byles&email=pavelbyles%40gmail.com";
    let response = client
        .post(&format!("{}/health/health_post_check", &app.address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(200, response.status().as_u16());

    /* let saved = sqlx::query!("SELECT id, name FROM villas",)
        .fetch_one(&mut db_connection)
        .await
        .expect("Failed to fetch saved villa.");

    assert_eq!(saved.name, "pavel byles") */
}

#[tokio::test]
async fn post_request_without_data_returns_400_test() {
    let app = common::spawn_app().await;
    let client = reqwest::Client::new();

    let test_cases = vec![
        ("name=pavel%20byles", "missing email"),
        ("email=pavelbyles%40gmail.com", "missing name"),
        ("", "missing name and email"),
    ];

    for (body, error_message) in test_cases {
        let response = client
            .post(&format!("{}/health/health_post_check", &app.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request");

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 Bad Request when the payload was {}.",
            error_message
        );
    }
}
