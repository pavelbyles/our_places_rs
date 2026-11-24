use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::{Error, HttpMessage};
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
        init_filter_only();
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

/// A reloadable filter with no output, for a process that never called
/// [`init_subscriber`]: in practice a test calling the loggers actuator. The
/// full subscriber would print every later log line of the test binary to
/// stdout, which the test harness does not capture.
fn init_filter_only() {
    let default_filter = "info".to_string();
    let (filter_layer, reload_handle) =
        tracing_subscriber::reload::Layer::new(EnvFilter::new(&default_filter));
    let _ = RELOAD_HANDLE.set(reload_handle);
    let _ = CURRENT_FILTER.set(RwLock::new(default_filter));
    let _ = tracing_subscriber::registry().with(filter_layer).try_init();
}

/// Retrieves the current active filter directive string.
pub fn get_current_filter() -> String {
    CURRENT_FILTER
        .get()
        .map(|lock| lock.read().clone())
        .unwrap_or_else(|| "info".to_string())
}

/// Builds each request's root span.
pub struct CustomRootSpanBuilder;

impl RootSpanBuilder for CustomRootSpanBuilder {
    /// Continues the caller's trace from `traceparent` (portals, other
    /// services) or `X-Cloud-Trace-Context` (Cloud Run's front end), or
    /// starts a new one, and records its IDs on the root span for logs.
    ///
    /// Builds the same span as `tracing_actix_web::root_span!`, except that
    /// `http.target` is the path alone: a query string can hold what a guest
    /// typed. The portals' request tracing records it the same way.
    fn on_request_start(request: &ServiceRequest) -> Span {
        let method = request.method().to_string();
        let route = request
            .match_pattern()
            .unwrap_or_else(|| "default".to_string());
        let connection_info = request.connection_info();
        let request_id = request
            .extensions()
            .get::<tracing_actix_web::RequestId>()
            .map(ToString::to_string)
            .unwrap_or_default();
        let user_agent = request
            .headers()
            .get("User-Agent")
            .map(|h| h.to_str().unwrap_or(""))
            .unwrap_or("");
        let flavor = format!("{:?}", request.version());
        let span = tracing::info_span!(
            "HTTP request",
            http.method = %method,
            http.route = %route,
            http.flavor = %flavor.trim_start_matches("HTTP/"),
            http.scheme = %connection_info.scheme(),
            http.host = %connection_info.host(),
            http.client_ip = %connection_info.realip_remote_addr().unwrap_or(""),
            http.user_agent = %user_agent,
            http.target = %request.path(),
            http.status_code = tracing::field::Empty,
            otel.name = %format!("{method} {route}"),
            otel.kind = "server",
            otel.status_code = tracing::field::Empty,
            trace_id = tracing::field::Empty,
            request_id = %request_id,
            exception.message = tracing::field::Empty,
            exception.details = tracing::field::Empty,
            span_id = tracing::field::Empty,
            trace_sampled = tracing::field::Empty
        );
        drop(connection_info);
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
