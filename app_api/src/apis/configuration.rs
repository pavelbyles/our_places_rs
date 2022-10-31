use actix_web::{get, web};
use serde::Serialize;
use std::env;

#[derive(Serialize, Debug)]
struct Config {
    target: String,
    port: u16,
}

#[get("/cfg")]
async fn config() -> web::Json<Config> {
    //let mut res_body = "Config vars: ".to_string();
    let cfg = Config {
        target: match env::var("TARGET") {
            Ok(env_target) => env_target,
            Err(_e) => "".to_string(),
        },
        port: 8080,
    };

    web::Json(cfg)
}
