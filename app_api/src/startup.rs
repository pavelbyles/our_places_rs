//! This configures all API routes and starts the web server

use actix_web::dev::Server;
use actix_web::http::header::ACCEPT;
use actix_web::{guard, web, App, Error, HttpRequest, HttpServer};
use sqlx::PgPool;
use std::net::TcpListener;

use crate::apis::{app, configuration, health, listings};

/// Runs web server and initialise API routes
pub fn run(listener: TcpListener, db_pool: PgPool) -> Result<Server, std::io::Error> {
    let db_pool = web::Data::new(db_pool);

    let server = HttpServer::new(move || {
        App::new()
            .route("/hello", web::get().to(app::greet_no_name))
            .route("/hello/{name}", web::get().to(app::greet_with_name))
            .route(
                "/db/getbannerurls",
                web::get().to(app::get_banner_image_urls),
            )
            .route("/health/health_check", web::get().to(health::health_check))
            .route(
                "/health/health_post_check",
                web::post().to(health::health_post_check),
            )
            .route("/cfg", web::get().to(configuration::config))
            .route(
                "/listings",
                web::post().guard(accept_guard).to(listings::create_listing),
            )
            .app_data(db_pool.clone())
    })
    .listen(listener)?
    .run();
    Ok(server)
}

/// This prevents any unacceptable request formats being requested in ACCEPT header
fn accept_guard(ctx: &guard::GuardContext<'_>) -> bool {
    let headers = ctx.head().headers();
    let accept_header = headers.get(ACCEPT);
    let supported_formats = vec![
        "application/json",
        "application/xml",
        "application/x-www-form-urlencoded",
    ]; // Supported formats

    match accept_header {
        Some(value) => {
            for mime in value.to_str().unwrap().split(',') {
                if supported_formats.iter().any(|f| f.trim() == mime) {
                    // return Ok(());
                    return true;
                }
            }
            false
        }
        None => false,
    }
}
