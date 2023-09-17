#[macro_use]
extern crate lazy_static;
use actix_web::{web, App, HttpServer};

mod apis;
mod settings;
mod util;

lazy_static! {
    static ref CONFIG: settings::Settings =
        settings::Settings::new().expect("config can be loaded");
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let result = log4rs::init_file("log4rs.yml", Default::default());
    match result {
        Ok(_) => {
            log::info!("Log config file loaded");
        }
        Err(error) => {
            println!("Could not load log4rs.yml: {}", error);
        }
    }

    let port: u16 = util::sys::get_port(CONFIG.server.port);
    let address = format!("{}:{}", CONFIG.server.host, port);

    log::info!("Starting server on port: {}", CONFIG.server.port);

    println!("Env is: {}", CONFIG.env);

    HttpServer::new(|| {
        App::new()
            .service(web::resource("/hello").route(web::get().to(apis::app::greet_no_name)))
            .service(
                web::resource("/hello/{name}").route(web::get().to(apis::app::greet_with_name)),
            )
            .service(
                web::resource("/db/getbannerurls")
                    .route(web::get().to(apis::app::get_banner_image_urls)),
            )
            .service(web::resource("/ping").route(web::get().to(apis::health::health_check)))
            .service(web::resource("/cfg").route(web::get().to(apis::configuration::config)))
    })
    .bind(address.clone())?
    .run()
    .await
}
