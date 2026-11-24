use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::{Error, HttpResponse, web};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use once_cell::sync::OnceCell;
use sqlx::PgPool;
use std::time::Instant;

static RECORDER_HANDLE: OnceCell<PrometheusHandle> = OnceCell::new();

/// Initializes the global Prometheus metrics recorder once and returns the handle.
pub fn init_metrics_recorder() -> PrometheusHandle {
    RECORDER_HANDLE
        .get_or_init(|| {
            PrometheusBuilder::new()
                .install_recorder()
                .unwrap_or_else(|_| PrometheusBuilder::new().build_recorder().handle())
        })
        .clone()
}

/// Updates PostgreSQL connection pool gauges for Prometheus exposition.
pub fn record_db_pool_metrics(pool: &PgPool) {
    let size = pool.size() as f64;
    let idle = pool.num_idle() as f64;
    let active = pool.size().saturating_sub(pool.num_idle() as u32) as f64;

    metrics::gauge!("db_connections_active").set(active);
    metrics::gauge!("db_connections_idle").set(idle);
    metrics::gauge!("db_connections_max").set(size);
}

/// Actix middleware intercepting requests to record request count and duration histogram.
pub async fn metrics_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let start = Instant::now();
    let method = req.method().as_str().to_string();

    // Use normalized route pattern to protect against cardinality explosions
    let path = req
        .match_pattern()
        .unwrap_or_else(|| req.path().to_string());

    let res = next.call(req).await;

    let status = match &res {
        Ok(resp) => resp.status().as_u16().to_string(),
        Err(err) => err.error_response().status().as_u16().to_string(),
    };

    let duration_secs = start.elapsed().as_secs_f64();

    metrics::counter!(
        "http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status.clone()
    )
    .increment(1);

    metrics::histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "path" => path,
        "status" => status
    )
    .record(duration_secs);

    res
}

/// Prometheus exposition endpoint handler returning text format 0.0.4.
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "actuator",
    responses(
        (status = 200, description = "Prometheus text metrics", body = String)
    )
)]
pub async fn metrics_handler(
    pool: Option<web::Data<PgPool>>,
    handle: Option<web::Data<PrometheusHandle>>,
) -> HttpResponse {
    if let Some(p) = pool {
        record_db_pool_metrics(p.get_ref());
    }

    let rendered = match handle {
        Some(h) => h.render(),
        None => init_metrics_recorder().render(),
    };

    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(rendered)
}
