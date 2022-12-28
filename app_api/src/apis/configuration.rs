use actix_web::{HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::{env, str};

#[derive(Deserialize, Serialize, Debug)]
struct Config {
    target: String,
    port: u16,
}

static ENV_TARGET_VAR: &str = "TARGET";
static DEFAULT_PORT: u16 = 8080;

pub async fn config(_req: HttpRequest) -> HttpResponse {
    let cfg = Config {
        target: match env::var(ENV_TARGET_VAR) {
            Ok(env_target) => {
                log::info!("App configs: Target {:?}", env_target);
                env_target
            }
            Err(e) => {
                log::error!("Error setting target: {}", e);
                ENV_TARGET_VAR.to_string()
            }
        },
        port: DEFAULT_PORT,
    };

    HttpResponse::Ok().json(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{body::to_bytes, http::StatusCode, test};

    // TODO: Change this to remove unwraps
    #[actix_web::test]
    async fn test_cfg_ok() {
        let test_env_var_val: &str = "test";

        env::set_var(ENV_TARGET_VAR, test_env_var_val);

        let req = test::TestRequest::default().to_http_request();
        let http_resp = config(req).await;
        assert_eq!(http_resp.status(), StatusCode::OK);

        let body_bytes = to_bytes(http_resp.into_body()).await.unwrap();
        let config_resp: Config =
            serde_json::from_str(str::from_utf8(&body_bytes).unwrap()).unwrap();
        assert_eq!(config_resp.target, test_env_var_val);
        assert_eq!(config_resp.port, 8080);
    }
}
