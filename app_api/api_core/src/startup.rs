//! This configures all API routes and starts the web server

use actix_web::dev::Server;
use actix_web::{App, HttpServer, web};
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

use crate::settings::Settings;

use actix_cors::Cors;

/// Runs web server and initialise API routes
pub fn run<F>(
    listener: TcpListener,
    db_pool: PgPool,
    config_fn: F,
    settings: Settings,
) -> Result<Server, std::io::Error>
where
    F: FnOnce(&mut web::ServiceConfig) + Clone + Send + 'static,
{
    let db_pool = web::Data::new(db_pool);
    let settings = web::Data::new(settings);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(Cors::permissive())
            .wrap(TracingLogger::default())
            .configure(config_fn.clone())
            .app_data(db_pool.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
