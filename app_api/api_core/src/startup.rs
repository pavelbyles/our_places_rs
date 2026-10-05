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
    let prometheus_handle = crate::actuator::init_metrics_recorder();
    let db_pool = web::Data::new(db_pool);
    let settings = web::Data::new(settings);
    let metrics_handle = web::Data::new(prometheus_handle);

    let server = HttpServer::new(move || {
        App::new()
            .wrap(Cors::permissive())
            .wrap(crate::actuator::ActuatorMetricsMiddleware)
            .wrap(TracingLogger::<crate::tracing_utils::CustomRootSpanBuilder>::new())
            .app_data(db_pool.clone())
            .app_data(settings.clone())
            .app_data(metrics_handle.clone())
            .configure(crate::actuator::configure_actuator)
            .configure(config_fn.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
