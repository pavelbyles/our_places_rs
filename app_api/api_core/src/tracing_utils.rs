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

/// Installs logging and tracing for `service_name` (the crate name). Spans
/// are propagated and, with `TRACE_EXPORTER=gcp`, exported to Cloud Trace;
/// see [`common::telemetry`].
pub fn init_subscriber(service_name: &'static str) {
    let default_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter = EnvFilter::try_new(&default_filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let (filter_layer, reload_handle) = tracing_subscriber::reload::Layer::new(env_filter);

    let _ = RELOAD_HANDLE.set(reload_handle);
    let _ = CURRENT_FILTER.set(RwLock::new(default_filter));

    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    let registry = tracing_subscriber::registry()
        .with(filter_layer)
        .with(common::telemetry::layer(service_name));

    match log_format.to_lowercase().as_str() {
        // Cloud Logging format; no span open/close lines, since each event
        // already carries its spans' fields (Cloud Run logs request latency).
        "json" => {
            let _ = registry.with(common::cloud_logging::layer()).try_init();
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
    common::telemetry::log_status();
}

/// Flushes buffered spans to Cloud Trace. Call once, after the server stops.
pub async fn shutdown_telemetry() {
    if let Err(e) = tokio::task::spawn_blocking(common::telemetry::shutdown).await {
        tracing::warn!(error = %e, "Trace exporter shutdown task failed");
    }
}

/// Dynamically updates the global tracing subscriber filter at runtime.
pub fn reload_filter(filter_directive: &str) -> Result<(), String> {
    if RELOAD_HANDLE.get().is_none() {
        init_subscriber(env!("CARGO_PKG_NAME"));
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
    /// Continues the caller's trace from `traceparent` (portals, other
    /// services) or `X-Cloud-Trace-Context` (Cloud Run's front end), or
    /// starts a new one, and records its IDs on the root span for logs.
    fn on_request_start(request: &ServiceRequest) -> Span {
        let span = tracing_actix_web::root_span!(
            request,
            span_id = tracing::field::Empty,
            trace_sampled = tracing::field::Empty
        );
        common::telemetry::continue_trace(&span, |name| {
            request
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        });
        span
    }

    fn on_request_end<B: MessageBody>(span: Span, outcome: &Result<ServiceResponse<B>, Error>) {
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}
