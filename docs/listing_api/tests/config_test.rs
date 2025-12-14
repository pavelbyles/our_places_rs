mod common;

#[tokio::test]
async fn integration_test_config() {
    let app = common::spawn_app().await;
    let client = reqwest::Client::new();
    let response = client
        .get(&format!("{}/cfg", &app.address))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());

    let body = response
        .text()
        .await
        .expect("Couldn't get text from response body");

    let config_response: our_places_app_api_listing_rs::apis::configuration::Config =
        serde_json::from_str(&body).unwrap();

    assert_ne!(config_response.target.len(), 0);
}
