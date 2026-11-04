//! Distributed tracing: one trace across the portals, the APIs and the workers.
//!
//! - **Propagation.** Trace context travels as W3C `traceparent` and Google's
//!   `X-Cloud-Trace-Context`, on HTTP calls ([`current_trace_headers`]) and
//!   on Pub/Sub messages and uploaded objects as attributes/metadata.
//!   [`continue_trace`] picks it up on the receiving side.
//! - **Logs.** [`continue_trace`] records `trace_id`, `span_id` and
//!   `trace_sampled` on the span; [`crate::cloud_logging`] turns them into
//!   Cloud Logging's trace fields, so every log line links to its trace.
//! - **Export.** With `TRACE_EXPORTER=gcp`, spans go to Cloud Trace through
//!   the Telemetry API's OTLP endpoint, authenticated with the service
//!   account's token from the metadata server. Otherwise nothing is
//!   exported, but IDs are still generated and propagated.
//!
//! Sampling is `TraceIdRatioBased` (`TRACE_SAMPLE_RATIO`, default 1.0). It
//! is decided from the trace ID alone, so every service makes the same
//! choice for a given trace.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use opentelemetry::propagation::text_map_propagator::FieldIter;
use opentelemetry::propagation::{
    Extractor, Injector, TextMapCompositePropagator, TextMapPropagator,
};
use opentelemetry::trace::{
    SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState, TracerProvider as _,
};
use opentelemetry::{Context, KeyValue, global};
use opentelemetry_http::{Bytes, HttpClient, HttpError, Request, Response};
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider, Tracer};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::registry::LookupSpan;

/// `gcp` turns on export to Cloud Trace; anything else (or unset) exports nothing.
pub const TRACE_EXPORTER_ENV: &str = "TRACE_EXPORTER";
/// Fraction of traces to record, 0.0–1.0. Default 1.0.
pub const TRACE_SAMPLE_RATIO_ENV: &str = "TRACE_SAMPLE_RATIO";
const PROJECT_ENV: &str = "GOOGLE_CLOUD_PROJECT";

const CLOUD_TRACE_OTLP_ENDPOINT: &str = "https://telemetry.googleapis.com/v1/traces";
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

/// Header (and attribute) names that carry trace context.
pub const TRACEPARENT: &str = "traceparent";
const TRACESTATE: &str = "tracestate";
const CLOUD_TRACE_HEADER: &str = "x-cloud-trace-context";
const TRACE_HEADERS: [&str; 3] = [TRACEPARENT, TRACESTATE, CLOUD_TRACE_HEADER];

/// Span fields [`continue_trace`] fills in. Declare them `Empty` on any span
/// passed to it, or the values are dropped.
pub const TRACE_ID_FIELD: &str = "trace_id";
/// Span field holding the span ID.
pub const SPAN_ID_FIELD: &str = "span_id";
/// Span field saying whether the trace is sampled.
pub const TRACE_SAMPLED_FIELD: &str = "trace_sampled";

static PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();
static STATUS: OnceLock<ExportStatus> = OnceLock::new();

#[derive(Debug)]
enum ExportStatus {
    Off,
    Exporting { ratio: f64 },
    Failed(String),
}

/// Installs the global propagator and tracer provider, and returns the layer
/// that turns `tracing` spans into OpenTelemetry spans. Call it once while
/// building the subscriber, then [`log_status`] once logging works, and
/// [`shutdown`] before exiting so buffered spans are sent.
pub fn layer<S>(service_name: &'static str) -> tracing_opentelemetry::OpenTelemetryLayer<S, Tracer>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    global::set_text_map_propagator(propagator());
    let provider = PROVIDER
        .get_or_init(|| build_provider(service_name))
        .clone();
    let tracer = provider.tracer(service_name);
    global::set_tracer_provider(provider);
    tracing_opentelemetry::layer().with_tracer(tracer)
}

/// Logs whether spans are being exported. Call after the subscriber is
/// installed; a misconfigured exporter is reported here as an error.
pub fn log_status() {
    match STATUS.get() {
        Some(ExportStatus::Exporting { ratio }) => {
            tracing::info!(sample_ratio = ratio, "Exporting traces to Cloud Trace")
        }
        Some(ExportStatus::Failed(reason)) => tracing::error!(
            reason = %reason,
            "Trace export is configured but could not start; traces are propagated but not exported"
        ),
        Some(ExportStatus::Off) | None => {
            tracing::debug!("Trace export is off ({TRACE_EXPORTER_ENV} is not 'gcp')")
        }
    }
}

/// Flushes buffered spans and stops the exporter thread. Blocks for up to
/// a few seconds, so call it from `spawn_blocking` in async code.
pub fn shutdown() {
    if let Some(provider) = PROVIDER.get()
        && let Err(e) = provider.shutdown_with_timeout(Duration::from_secs(5))
    {
        tracing::warn!(error = %e, "Trace exporter did not shut down cleanly");
    }
}

/// Makes `span` continue the trace described by the incoming headers or
/// attributes (looked up by name through `get`), or start a new one, and
/// records its IDs for log correlation. Call it right after creating the
/// span, before entering it or creating children.
pub fn continue_trace(span: &Span, get: impl Fn(&str) -> Option<String>) {
    let carrier: HashMap<String, String> = TRACE_HEADERS
        .iter()
        .filter_map(|name| get(name).map(|value| ((*name).to_owned(), value)))
        .collect();
    if !carrier.is_empty() {
        let parent = global::get_text_map_propagator(|p| p.extract(&carrier));
        if parent.span().span_context().is_valid() {
            // Fails only if the span was already started or no OTel layer
            // is installed; the span then simply begins a new trace.
            let _ = span.set_parent(parent);
        }
    }
    record_ids(span);
}

/// The trace headers for the current span, to send with an outgoing HTTP
/// request or attach to a message. Empty when there is no active trace.
pub fn current_trace_headers() -> HashMap<String, String> {
    let cx = Span::current().context();
    let mut carrier = HashMap::new();
    global::get_text_map_propagator(|p| p.inject_context(&cx, &mut carrier));
    carrier
}

fn record_ids(span: &Span) {
    let cx = span.context();
    let otel_span = cx.span();
    let sc = otel_span.span_context();
    if !sc.is_valid() {
        return;
    }
    span.record(TRACE_ID_FIELD, sc.trace_id().to_string());
    span.record(SPAN_ID_FIELD, sc.span_id().to_string());
    // Only claim a sampled trace when it is actually being exported, so Cloud
    // Logging doesn't link to traces that don't exist.
    if matches!(STATUS.get(), Some(ExportStatus::Exporting { .. })) {
        span.record(TRACE_SAMPLED_FIELD, sc.is_sampled());
    }
}

fn propagator() -> TextMapCompositePropagator {
    // Later propagators win on extract, so `traceparent` beats the Google header.
    TextMapCompositePropagator::new(vec![
        Box::new(CloudTraceContextPropagator::default()),
        Box::new(TraceContextPropagator::new()),
    ])
}

fn build_provider(service_name: &'static str) -> SdkTracerProvider {
    let ratio = std::env::var(TRACE_SAMPLE_RATIO_ENV)
        .ok()
        .and_then(|r| r.trim().parse::<f64>().ok())
        .map_or(1.0, |r| r.clamp(0.0, 1.0));
    let project = std::env::var(PROJECT_ENV)
        .ok()
        .filter(|p| !p.trim().is_empty());

    let mut resource = Resource::builder().with_service_name(service_name);
    if let Some(project) = &project {
        // The Telemetry API routes spans by this attribute.
        resource = resource.with_attribute(KeyValue::new("gcp.project_id", project.clone()));
    }
    let builder = SdkTracerProvider::builder()
        .with_sampler(Sampler::TraceIdRatioBased(ratio))
        .with_resource(resource.build());

    let wants_export =
        std::env::var(TRACE_EXPORTER_ENV).is_ok_and(|v| v.eq_ignore_ascii_case("gcp"));
    let (builder, status) = match (wants_export, project) {
        (false, _) => (builder, ExportStatus::Off),
        (true, None) => (
            builder,
            ExportStatus::Failed(format!("{TRACE_EXPORTER_ENV}=gcp needs {PROJECT_ENV}")),
        ),
        (true, Some(_)) => match cloud_trace_exporter() {
            Ok(exporter) => (
                builder.with_batch_exporter(exporter),
                ExportStatus::Exporting { ratio },
            ),
            Err(e) => (builder, ExportStatus::Failed(e.to_string())),
        },
    };
    let _ = STATUS.set(status);
    builder.build()
}

fn cloud_trace_exporter()
-> Result<opentelemetry_otlp::SpanExporter, opentelemetry_otlp::ExporterBuildError> {
    opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(CLOUD_TRACE_OTLP_ENDPOINT)
        .with_timeout(Duration::from_secs(10))
        .with_http_client(MetadataAuthClient::default())
        .build()
}

/// Google's `X-Cloud-Trace-Context: TRACE_ID/SPAN_ID;o=FLAG`, where the span
/// ID is decimal. Cloud Run's front end adds it to incoming requests.
#[derive(Debug)]
struct CloudTraceContextPropagator {
    fields: Vec<String>,
}

impl Default for CloudTraceContextPropagator {
    fn default() -> Self {
        Self {
            fields: vec![CLOUD_TRACE_HEADER.to_owned()],
        }
    }
}

impl TextMapPropagator for CloudTraceContextPropagator {
    fn inject_context(&self, cx: &Context, injector: &mut dyn Injector) {
        let span = cx.span();
        let sc = span.span_context();
        if sc.is_valid() {
            injector.set(
                CLOUD_TRACE_HEADER,
                format!(
                    "{}/{};o={}",
                    sc.trace_id(),
                    u64::from_be_bytes(sc.span_id().to_bytes()),
                    u8::from(sc.is_sampled())
                ),
            );
        }
    }

    fn extract_with_context(&self, cx: &Context, extractor: &dyn Extractor) -> Context {
        extractor
            .get(CLOUD_TRACE_HEADER)
            .and_then(parse_cloud_trace_context)
            .map_or_else(|| cx.clone(), |sc| cx.with_remote_span_context(sc))
    }

    fn fields(&self) -> FieldIter<'_> {
        FieldIter::new(&self.fields)
    }
}

fn parse_cloud_trace_context(value: &str) -> Option<SpanContext> {
    let (trace, rest) = value.trim().split_once('/')?;
    let (span, options) = rest.split_once(';').unwrap_or((rest, ""));
    let trace_id = TraceId::from_hex(trace).ok()?;
    let span_id = SpanId::from_bytes(span.parse::<u64>().ok()?.to_be_bytes());
    if trace_id == TraceId::INVALID || span_id == SpanId::INVALID {
        return None;
    }
    let flags = if options.trim() == "o=1" {
        TraceFlags::SAMPLED
    } else {
        TraceFlags::default()
    };
    Some(SpanContext::new(
        trace_id,
        span_id,
        flags,
        true,
        TraceState::default(),
    ))
}

/// Sends OTLP requests with the service account's OAuth token. Runs on the
/// batch exporter's own thread, so it uses a blocking client, created on
/// first use so startup does no extra work.
#[derive(Debug, Default)]
struct MetadataAuthClient {
    client: OnceLock<reqwest::blocking::Client>,
    token: Mutex<Option<(String, Instant)>>,
}

#[derive(serde::Deserialize)]
struct AccessToken {
    access_token: String,
    expires_in: u64,
}

impl MetadataAuthClient {
    fn client(&self) -> Result<&reqwest::blocking::Client, HttpError> {
        if let Some(client) = self.client.get() {
            return Ok(client);
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(self.client.get_or_init(|| client))
    }

    fn access_token(&self, client: &reqwest::blocking::Client) -> Result<String, HttpError> {
        let mut cached = self
            .token
            .lock()
            .map_err(|_| "trace exporter token cache is poisoned")?;
        if let Some((token, expires)) = cached.as_ref()
            && Instant::now() < *expires
        {
            return Ok(token.clone());
        }
        let fresh: AccessToken = client
            .get(METADATA_TOKEN_URL)
            .header("Metadata-Flavor", "Google")
            .send()?
            .error_for_status()?
            .json()?;
        let expires = Instant::now() + Duration::from_secs(fresh.expires_in.saturating_sub(60));
        *cached = Some((fresh.access_token.clone(), expires));
        Ok(fresh.access_token)
    }

    fn send(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        let client = self.client()?;
        let token = self.access_token(client)?;
        let (parts, body) = request.into_parts();
        let response = client
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .bearer_auth(token)
            .body(body.to_vec())
            .send()?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes()?;
        if !status.is_success() {
            tracing::warn!(
                %status,
                body = %String::from_utf8_lossy(&body),
                "Cloud Trace rejected a span batch"
            );
        }
        let mut out = Response::new(body);
        *out.status_mut() = status;
        *out.headers_mut() = headers;
        Ok(out)
    }
}

#[async_trait::async_trait]
impl HttpClient for MetadataAuthClient {
    async fn send_bytes(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        self.send(request).inspect_err(|e| {
            tracing::warn!(error = %e, "Could not send spans to Cloud Trace");
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cloud_trace_header() {
        let sc =
            parse_cloud_trace_context("105445aa7843bc8bf206b12000100000/1;o=1").expect("valid");
        assert_eq!(
            sc.trace_id().to_string(),
            "105445aa7843bc8bf206b12000100000"
        );
        assert_eq!(sc.span_id(), SpanId::from_bytes(1u64.to_be_bytes()));
        assert!(sc.is_sampled());
        assert!(sc.is_remote());
    }

    #[test]
    fn cloud_trace_header_without_options_is_unsampled() {
        let sc = parse_cloud_trace_context("105445aa7843bc8bf206b12000100000/42").expect("valid");
        assert!(!sc.is_sampled());
    }

    #[test]
    fn rejects_malformed_cloud_trace_headers() {
        for bad in [
            "",
            "nope",
            "105445aa7843bc8bf206b12000100000",
            "zz/1;o=1",
            "105445aa7843bc8bf206b12000100000/abc",
            "00000000000000000000000000000000/1;o=1",
            "105445aa7843bc8bf206b12000100000/0;o=1",
        ] {
            assert!(parse_cloud_trace_context(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn cloud_trace_header_round_trips() {
        let propagator = CloudTraceContextPropagator::default();
        let mut carrier = HashMap::new();
        carrier.insert(
            CLOUD_TRACE_HEADER.to_owned(),
            "105445aa7843bc8bf206b12000100000/12345;o=1".to_owned(),
        );
        let cx = propagator.extract(&carrier);
        let mut out = HashMap::new();
        propagator.inject_context(&cx, &mut out);
        assert_eq!(
            out.get(CLOUD_TRACE_HEADER).map(String::as_str),
            Some("105445aa7843bc8bf206b12000100000/12345;o=1")
        );
    }

    #[test]
    fn traceparent_wins_over_cloud_trace_header() {
        let mut carrier = HashMap::new();
        carrier.insert(
            CLOUD_TRACE_HEADER.to_owned(),
            "105445aa7843bc8bf206b12000100000/1;o=1".to_owned(),
        );
        carrier.insert(
            TRACEPARENT.to_owned(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_owned(),
        );
        let cx = propagator().extract(&carrier);
        assert_eq!(
            cx.span().span_context().trace_id().to_string(),
            "4bf92f3577b34da6a3ce929d0e0e4736"
        );
    }

    #[test]
    fn continue_trace_joins_the_callers_trace_and_propagates_it() {
        use tracing_subscriber::layer::SubscriberExt;

        let subscriber = tracing_subscriber::registry().with(layer("telemetry-test"));
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                "request",
                trace_id = tracing::field::Empty,
                span_id = tracing::field::Empty,
                trace_sampled = tracing::field::Empty
            );
            continue_trace(&span, |name| {
                (name == TRACEPARENT)
                    .then(|| "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_owned())
            });
            let _entered = span.enter();
            let headers = current_trace_headers();
            let traceparent = headers.get(TRACEPARENT).expect("traceparent injected");
            assert!(
                traceparent.starts_with("00-4bf92f3577b34da6a3ce929d0e0e4736-"),
                "{traceparent}"
            );
            // A new span ID for this hop, not the caller's.
            assert!(!traceparent.contains("00f067aa0ba902b7"), "{traceparent}");
            assert!(
                headers
                    .get(CLOUD_TRACE_HEADER)
                    .is_some_and(|v| v.starts_with("4bf92f3577b34da6a3ce929d0e0e4736/"))
            );
        });
    }
}
