pub mod health;
pub mod info;
pub mod loggers;
pub mod metrics;
pub mod models;

#[cfg(test)]
mod actuator_test;

use actix_web::web;

pub use health::{health_check, liveness_check, readiness_check, startup_check};
pub use info::info_handler;
pub use loggers::{get_logger_by_name_handler, get_loggers_handler, update_logger_handler};
pub use metrics::{init_metrics_recorder, metrics_handler, metrics_middleware};
pub use models::*;

/// Helper struct for type-safe middleware wrapping in `startup::run`.
pub struct ActuatorMetricsMiddleware;

impl<S, B> actix_web::dev::Transform<S, actix_web::dev::ServiceRequest>
    for ActuatorMetricsMiddleware
where
    S: actix_web::dev::Service<
            actix_web::dev::ServiceRequest,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        > + 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = actix_web::dev::ServiceResponse<B>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = ActuatorMetricsMiddlewareService<S>;
    type Future = std::future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(ActuatorMetricsMiddlewareService { service }))
    }
}

pub struct ActuatorMetricsMiddlewareService<S> {
    service: S,
}

impl<S, B> actix_web::dev::Service<actix_web::dev::ServiceRequest>
    for ActuatorMetricsMiddlewareService<S>
where
    S: actix_web::dev::Service<
            actix_web::dev::ServiceRequest,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        > + 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = actix_web::dev::ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + 'static>,
    >;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: actix_web::dev::ServiceRequest) -> Self::Future {
        let start = std::time::Instant::now();
        let method = req.method().as_str().to_string();
        let path = req
            .match_pattern()
            .unwrap_or_else(|| req.path().to_string());

        let fut = self.service.call(req);

        Box::pin(async move {
            let res = fut.await;

            let status = match &res {
                Ok(resp) => resp.status().as_u16().to_string(),
                Err(err) => err.error_response().status().as_u16().to_string(),
            };

            let duration_secs = start.elapsed().as_secs_f64();

            ::metrics::counter!(
                "http_requests_total",
                "method" => method.clone(),
                "path" => path.clone(),
                "status" => status.clone()
            )
            .increment(1);

            ::metrics::histogram!(
                "http_request_duration_seconds",
                "method" => method,
                "path" => path,
                "status" => status
            )
            .record(duration_secs);

            res
        })
    }
}

/// Configures all actuator routes on the service configuration.
pub fn configure_actuator(cfg: &mut web::ServiceConfig) {
    let _ = init_metrics_recorder();

    cfg.service(web::resource("/health").route(web::get().to(health::health_check)))
        .service(web::resource("/health/startup").route(web::get().to(health::startup_check)))
        .service(web::resource("/health/liveness").route(web::get().to(health::liveness_check)))
        .service(web::resource("/health/readiness").route(web::get().to(health::readiness_check)))
        .service(web::resource("/metrics").route(web::get().to(metrics::metrics_handler)))
        .service(web::resource("/info").route(web::get().to(info::info_handler)))
        .service(
            web::resource("/loggers")
                .route(web::get().to(loggers::get_loggers_handler))
                .route(web::post().to(|req, body| {
                    loggers::update_logger_handler(req, web::Path::from("ROOT".to_string()), body)
                })),
        )
        .service(
            web::resource("/loggers/{name}")
                .route(web::get().to(loggers::get_logger_by_name_handler))
                .route(web::post().to(loggers::update_logger_handler)),
        );
}
