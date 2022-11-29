use actix_web::{HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::{env, str};

#[derive(Deserialize, Serialize, Debug)]
struct Config {
    target: String,
    port: u16,
}

pub async fn config(_req: HttpRequest) -> HttpResponse {
    let cfg = Config {
        target: match env::var("TARGET") {
            Ok(env_target) => env_target,
            Err(_e) => "".to_string(),
        },
        port: 8080,
    };

    HttpResponse::Ok().json(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{body::to_bytes, http::StatusCode, test};

    #[actix_web::test]
    async fn test_cfg_ok() {
        env::set_var("TARGET", "test");

        let req = test::TestRequest::default().to_http_request();
        let http_resp = config(req).await;

        assert_eq!(http_resp.status(), StatusCode::OK);

        let body_bytes = to_bytes(http_resp.into_body()).await.unwrap();
        let config_resp: Config =
            serde_json::from_str(str::from_utf8(&body_bytes).unwrap()).unwrap();
        assert_eq!(config_resp.target, "test".to_string());
        assert_eq!(config_resp.port, 8080);
    }
}
