use actix_web::Error;
use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use tracing::Span;
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};
use tracing_subscriber::{
    EnvFilter, Registry, layer::SubscriberExt, reload::Handle, util::SubscriberInitExt,
};

static RELOAD_HANDLE: OnceCell<Handle<EnvFilter, Registry>> = OnceCell::new();
static CURRENT_FILTER: OnceCell<RwLock<String>> = OnceCell::new();

pub fn init_subscriber() {
    let default_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter = EnvFilter::try_new(&default_filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let (filter_layer, reload_handle) = tracing_subscriber::reload::Layer::new(env_filter);

    let _ = RELOAD_HANDLE.set(reload_handle);
    let _ = CURRENT_FILTER.set(RwLock::new(default_filter));

    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    let registry = tracing_subscriber::registry().with(filter_layer);

    match log_format.to_lowercase().as_str() {
        "json" => {
            let _ = registry
                .with(tracing_subscriber::fmt::layer().json().with_span_events(
                    tracing_subscriber::fmt::format::FmtSpan::NEW
                        | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
                ))
                .try_init();
        }
        _ => {
            let _ = registry
                .with(tracing_subscriber::fmt::layer().with_span_events(
                    tracing_subscriber::fmt::format::FmtSpan::NEW
                        | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
                ))
                .try_init();
        }
    }
}

/// Dynamically updates the global tracing subscriber filter at runtime.
pub fn reload_filter(filter_directive: &str) -> Result<(), String> {
    if RELOAD_HANDLE.get().is_none() {
        init_subscriber();
    }

    let handle = RELOAD_HANDLE
        .get()
        .ok_or_else(|| "Tracing reload handle not initialized".to_string())?;
    let new_filter = EnvFilter::try_new(filter_directive).map_err(|e| e.to_string())?;

    let _ = handle.reload(new_filter);

    if let Some(current) = CURRENT_FILTER.get() {
        *current.write() = filter_directive.to_string();
    }

    Ok(())
}

/// Retrieves the current active filter directive string.
pub fn get_current_filter() -> String {
    CURRENT_FILTER
        .get()
        .map(|lock| lock.read().clone())
        .unwrap_or_else(|| "info".to_string())
}

pub struct CustomRootSpanBuilder;

impl RootSpanBuilder for CustomRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        // 1. Google Cloud Run header: X-Cloud-Trace-Context: TRACE_ID/SPAN_ID;o=TRACE_TRUE
        let cloud_trace_id = request
            .headers()
            .get("x-cloud-trace-context")
            .and_then(|h| h.to_str().ok())
            .and_then(|val| val.split('/').next())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // 2. W3C Trace Context: traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
        let w3c_trace_id = request
            .headers()
            .get("traceparent")
            .and_then(|h| h.to_str().ok())
            .and_then(|val| {
                let parts: Vec<&str> = val.split('-').collect();
                if parts.len() >= 4 {
                    Some(parts[1].to_string())
                } else {
                    None
                }
            });

        // 3. Custom trace-id header or random UUID fallback
        let trace_id = cloud_trace_id
            .or(w3c_trace_id)
            .or_else(|| {
                request
                    .headers()
                    .get("trace-id")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        if !trace_id.is_empty() {
            tracing_actix_web::root_span!(request, trace_id = trace_id)
        } else {
            tracing_actix_web::root_span!(request)
        }
    }

    fn on_request_end<B: MessageBody>(span: Span, outcome: &Result<ServiceResponse<B>, Error>) {
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}
