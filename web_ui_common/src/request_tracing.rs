use std::env;

use tracing::Instrument;
use tracing::field::Empty;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use topcoat::context::Cx;
use topcoat::router::request::{headers, method, uri};
use topcoat::router::{Body, Layer, LayerFuture, Next, Path};

static LOG_FORMAT_ENV: &str = "LOG_FORMAT";

/// Installs logging and tracing for a portal: plain text locally, Cloud
/// Logging JSON when `LOG_FORMAT=json` (Cloud Run), and OpenTelemetry spans
/// in both (see [`common::telemetry`]).
pub fn init(service_name: &'static str) {
    let registry = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(common::telemetry::layer(service_name));
    if env::var(LOG_FORMAT_ENV).is_ok_and(|f| f.eq_ignore_ascii_case("json")) {
        registry.with(common::cloud_logging::layer()).init();
    } else {
        registry.with(tracing_subscriber::fmt::layer()).init();
    }
    common::telemetry::log_status();
}

/// Opens a server span per request that continues the trace Cloud Run's
/// front end started (`traceparent` / `X-Cloud-Trace-Context`). Calls to the
/// APIs made while handling the request carry it on, so one trace covers the
/// page and every API call behind it.
///
/// Register it last: Topcoat runs the most recently registered layer first,
/// so this one then wraps the others and the handler.
#[derive(Debug, Default, Clone, Copy)]
pub struct RequestTracing;

impl Layer for RequestTracing {
    fn path(&self) -> Option<&Path> {
        None
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        let method = method(cx).clone();
        let path = uri(cx).path().to_owned();
        let span = tracing::info_span!(
            "HTTP request",
            otel.name = %format!("{method} {path}"),
            otel.kind = "server",
            otel.status_code = Empty,
            http.method = %method,
            http.target = %path,
            http.status_code = Empty,
            trace_id = Empty,
            span_id = Empty,
            trace_sampled = Empty,
        );
        let request_headers = headers(cx);
        common::telemetry::continue_trace(&span, |name| {
            request_headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        });

        let recorder = span.clone();
        Box::pin(
            async move {
                let result = next.run(cx, body).await;
                match &result {
                    Ok(response) => {
                        let status = response.status();
                        recorder.record("http.status_code", status.as_u16());
                        if status.is_server_error() {
                            recorder.record("otel.status_code", "ERROR");
                        }
                    }
                    Err(_) => {
                        recorder.record("otel.status_code", "ERROR");
                    }
                }
                result
            }
            .instrument(span),
        )
    }
}
