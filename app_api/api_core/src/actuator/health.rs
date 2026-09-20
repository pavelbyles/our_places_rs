use actix_web::{HttpResponse, web};
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::actuator::models::{
    ComponentDetails, ComponentHealth, HealthResponse, HealthStatus, ProbeResponse,
};

/// Cloud Run Startup Probe: verifies process and connection pool initialization.
#[utoipa::path(
    get,
    path = "/health/startup",
    tag = "actuator",
    responses(
        (status = 200, description = "Application initialization complete", body = ProbeResponse),
        (status = 503, description = "Application still initializing or failed", body = ProbeResponse)
    )
)]
pub async fn startup_check(pool: Option<web::Data<PgPool>>) -> HttpResponse {
    match pool {
        Some(_) => HttpResponse::Ok().json(ProbeResponse {
            status: HealthStatus::Up,
        }),
        None => HttpResponse::ServiceUnavailable().json(ProbeResponse {
            status: HealthStatus::Down,
        }),
    }
}

/// Cloud Run Liveness Probe: verifies that the container process is responsive.
/// Strictly non-blocking and isolated from external backends to prevent restart loops.
#[utoipa::path(
    get,
    path = "/health/liveness",
    tag = "actuator",
    responses(
        (status = 200, description = "Container is alive", body = ProbeResponse)
    )
)]
pub async fn liveness_check() -> HttpResponse {
    HttpResponse::Ok().json(ProbeResponse {
        status: HealthStatus::Up,
    })
}

/// Cloud Run Readiness Probe: verifies database connectivity before routing traffic.
#[utoipa::path(
    get,
    path = "/health/readiness",
    tag = "actuator",
    responses(
        (status = 200, description = "Service ready to receive traffic", body = ProbeResponse),
        (status = 503, description = "Service dependencies unavailable", body = ProbeResponse)
    )
)]
pub async fn readiness_check(pool: Option<web::Data<PgPool>>) -> HttpResponse {
    match pool {
        Some(p) => {
            let check = tokio::time::timeout(
                Duration::from_secs(2),
                sqlx::query("SELECT 1").execute(p.get_ref()),
            )
            .await;

            match check {
                Ok(Ok(_)) => HttpResponse::Ok().json(ProbeResponse {
                    status: HealthStatus::Up,
                }),
                Ok(Err(e)) => {
                    tracing::warn!("Readiness probe DB query failed: {}", e);
                    HttpResponse::ServiceUnavailable().json(ProbeResponse {
                        status: HealthStatus::Down,
                    })
                }
                Err(_) => {
                    tracing::warn!("Readiness probe DB query timed out (2s)");
                    HttpResponse::ServiceUnavailable().json(ProbeResponse {
                        status: HealthStatus::Down,
                    })
                }
            }
        }
        None => HttpResponse::ServiceUnavailable().json(ProbeResponse {
            status: HealthStatus::Down,
        }),
    }
}

/// Composite Health Check: aggregates status of core subsystems with latency metadata.
#[utoipa::path(
    get,
    path = "/health",
    tag = "actuator",
    responses(
        (status = 200, description = "All system components healthy", body = HealthResponse),
        (status = 503, description = "One or more components degraded", body = HealthResponse)
    )
)]
pub async fn health_check(pool: Option<web::Data<PgPool>>) -> HttpResponse {
    let mut components = HashMap::new();
    let mut overall_healthy = true;

    match pool {
        Some(p) => {
            let start = Instant::now();
            let check = tokio::time::timeout(
                Duration::from_secs(2),
                sqlx::query("SELECT 1").execute(p.get_ref()),
            )
            .await;
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

            match check {
                Ok(Ok(_)) => {
                    components.insert(
                        "db".to_string(),
                        ComponentHealth {
                            status: HealthStatus::Up,
                            details: Some(ComponentDetails {
                                database: Some("PostgreSQL".to_string()),
                                latency_ms: Some((elapsed_ms * 100.0).round() / 100.0),
                                error: None,
                            }),
                        },
                    );
                }
                Ok(Err(e)) => {
                    overall_healthy = false;
                    tracing::warn!("Health check DB query failed: {}", e);
                    components.insert(
                        "db".to_string(),
                        ComponentHealth {
                            status: HealthStatus::Down,
                            details: Some(ComponentDetails {
                                database: Some("PostgreSQL".to_string()),
                                latency_ms: Some((elapsed_ms * 100.0).round() / 100.0),
                                error: Some("Connection query failed".to_string()),
                            }),
                        },
                    );
                }
                Err(_) => {
                    overall_healthy = false;
                    tracing::warn!("Health check DB query timed out (2s)");
                    components.insert(
                        "db".to_string(),
                        ComponentHealth {
                            status: HealthStatus::Down,
                            details: Some(ComponentDetails {
                                database: Some("PostgreSQL".to_string()),
                                latency_ms: Some(2000.0),
                                error: Some("Connection timed out (2s)".to_string()),
                            }),
                        },
                    );
                }
            }
        }
        None => {
            overall_healthy = false;
            components.insert(
                "db".to_string(),
                ComponentHealth {
                    status: HealthStatus::Down,
                    details: Some(ComponentDetails {
                        database: None,
                        latency_ms: None,
                        error: Some("Database pool not available".to_string()),
                    }),
                },
            );
        }
    }

    let status = if overall_healthy {
        HealthStatus::Up
    } else {
        HealthStatus::Down
    };

    let resp = HealthResponse { status, components };

    if overall_healthy {
        HttpResponse::Ok().json(resp)
    } else {
        HttpResponse::ServiceUnavailable().json(resp)
    }
}
