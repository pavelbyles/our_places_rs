#[macro_use]
extern crate lazy_static;
use actix_web::{App, HttpServer};

mod apis;
mod settings;
mod util;

lazy_static! {
    static ref CONFIG: settings::Settings =
        settings::Settings::new().expect("config can be loaded");
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = util::sys::get_port(CONFIG.server.port);
    let address = format!("{}:{}", CONFIG.server.host, port);

    HttpServer::new(|| {
        App::new()
            .service(apis::app::greet_no_name)
            .service(apis::app::greet_with_name)
            .service(apis::health::health_check)
            .service(apis::configuration::config)
    })
    .bind(address.clone())?
    .run()
    .await
}
