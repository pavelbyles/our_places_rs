use actix_web::{HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::str;

#[derive(Deserialize, Serialize, Debug)]
struct PingResponse {
    status: String,
}

pub async fn health_check(_req: HttpRequest) -> HttpResponse {
    let resp = PingResponse {
        status: "alive".to_string(),
    };

    HttpResponse::Ok().json(resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{body::to_bytes, http::StatusCode, test, web, App};

    #[actix_web::test]
    async fn test_ping_ok() {
        let req = test::TestRequest::default().to_http_request();
        let http_resp = health_check(req).await;

        assert_eq!(http_resp.status(), StatusCode::OK);
        let body_bytes = to_bytes(http_resp.into_body()).await.unwrap();
        let ping_resp: PingResponse =
            serde_json::from_str(str::from_utf8(&body_bytes).unwrap()).unwrap();
        assert_eq!(ping_resp.status, "alive".to_string())
    }

    #[actix_web::test]
    async fn test_ping_ok2() {
        let app = test::init_service(
            App::new().service(web::resource("/ping").route(web::get().to(health_check))),
        )
        .await;

        let req = test::TestRequest::get().uri("/ping").to_request();
        let http_resp = actix_web::dev::Service::call(&app, req).await.unwrap();

        assert_eq!(http_resp.status(), StatusCode::OK);
    }
}
