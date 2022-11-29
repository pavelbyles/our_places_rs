use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::str;

#[derive(Deserialize, Serialize, Debug)]
struct HelloResponse {
    response: String,
}

pub async fn greet_no_name(_req: HttpRequest) -> HttpResponse {
    let resp = HelloResponse {
        response: "Hello World!".to_string(),
    };

    HttpResponse::Ok().json(resp)
}

pub async fn greet_with_name(_req: HttpRequest, name: web::Path<String>) -> HttpResponse {
    let resp = HelloResponse {
        response: format!("Hello {}!", name),
    };

    HttpResponse::Ok().json(resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{body::to_bytes, http::StatusCode, test};

    #[actix_web::test]
    async fn test_greet_no_name_ok() {
        let req = test::TestRequest::default().to_http_request();
        let http_resp = greet_no_name(req).await;

        assert_eq!(http_resp.status(), StatusCode::OK);
        let body_bytes = to_bytes(http_resp.into_body()).await.unwrap();
        let hello_resp: HelloResponse =
            serde_json::from_str(str::from_utf8(&body_bytes).unwrap()).unwrap();
        assert_eq!(hello_resp.response, "Hello World!".to_string())
    }
}
