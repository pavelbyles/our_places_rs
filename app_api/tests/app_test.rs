mod common;

#[tokio::test]
async fn greet_no_name() {
    let app = common::spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/hello", &app.address))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());

    let body = response
        .text()
        .await
        .expect("Couldn't get text from response body");

    let hello_response: our_places_app_api_rs::apis::app::HelloResponse =
        serde_json::from_str(&body).unwrap();

    assert_eq!(hello_response.response.to_string(), "Hello World!");
}

#[tokio::test]
async fn greet_with_name() {
    let test_names = vec!["pavel", "kristina", "laila", "ethan"];

    let app = common::spawn_app().await;
    let client = reqwest::Client::new();

    for test_name in test_names {
        let response_body = client
            .get(&format!("{}/hello/{}", &app.address, test_name.to_string()))
            .send()
            .await
            .expect("Failed to execute request")
            .text()
            .await
            .expect("Couldn't get text from body");

        let hello_response: our_places_app_api_rs::apis::app::HelloResponse =
            serde_json::from_str(&response_body).unwrap();

        assert_eq!(
            hello_response.response.to_string(),
            format!("Hello {}!", test_name)
        );
    }
}
