use our_places_app_api_rs::apis::listings::StructureType;
use rust_decimal::Decimal;
mod common;

#[tokio::test]
async fn create_listing_with_data_returns_200_test() {
    let app = common::spawn_app().await;
    let client = reqwest::Client::new();

    let data = our_places_app_api_rs::apis::listings::ListingFormData {
        id: String::from("0"),
        name: String::from("Test rental name"),
        description: String::from("Test description"),
        structure: String::from("Apartment"),
        country: String::from("Jamaica"),
        price_per_night: Decimal::new(1, 0),
        is_active: false,
    };

    let body = serde_urlencoded::to_string(&data).expect("Failed to urlencode ListingFormData");
    println!("UrlEncoded body is: {0}", body);

    let response = client
        .post(&format!("{}/listings", &app.address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(200, response.status().as_u16());

    let saved = sqlx::query!("SELECT id, name FROM listing",)
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch saved villa.");

    assert_eq!(saved.name, String::from("Test rental name"));
}
