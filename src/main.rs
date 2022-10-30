use actix_web::{App, HttpServer};

mod apis;
pub mod util;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = util::sys::get_port(8080);
    let address = format!("0.0.0.0:{}", port);

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
