use actix_web::{get, web};
use serde::Serialize;

#[derive(Serialize, Debug)]
struct PingResponse {
    status: String,
}

#[get("/ping")]
async fn health_check() -> web::Json<PingResponse> {
    let resp = PingResponse {
        status: "alive".to_string(),
    };

    web::Json(resp)
}
/*
#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{
        http::{self, header::ContentType},
        test,
    };

    #[actix_web::test]
    async fn test_ping_ok() {
        let req = test::TestRequest::default()
            .insert_header(ContentType::plaintext())
            .to_http_request();
        let resp = crate::apis::health::health_check().await;
        assert_eq!(resp.status(), http::StatusCode::OK);
    }
}
*/
