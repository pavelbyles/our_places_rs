use actix_web::{get, web};
use serde::Serialize;

#[derive(Serialize, Debug)]
struct HelloResponse {
    response: String,
}

#[get("/hello")]
async fn greet_no_name() -> web::Json<HelloResponse> {
    let resp = HelloResponse {
        response: "Hello World!".to_string(),
    };

    web::Json(resp)
}

#[get("/hello/{name}")]
async fn greet_with_name(name: web::Path<String>) -> web::Json<HelloResponse> {
    let resp = HelloResponse {
        response: format!("Hello {}!", name),
    };

    web::Json(resp)
}
