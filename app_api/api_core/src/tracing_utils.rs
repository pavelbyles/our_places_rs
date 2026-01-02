use actix_web::Error;
use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use tracing::Span;
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_subscriber() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());

    let registry = tracing_subscriber::registry().with(env_filter);

    match log_format.to_lowercase().as_str() {
        "json" => {
            registry
                .with(tracing_subscriber::fmt::layer().json().with_span_events(
                    tracing_subscriber::fmt::format::FmtSpan::NEW
                        | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
                ))
                .init();
        }
        _ => {
            registry
                .with(tracing_subscriber::fmt::layer().with_span_events(
                    tracing_subscriber::fmt::format::FmtSpan::NEW
                        | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
                ))
                .init();
        }
    }
}

pub struct CustomRootSpanBuilder;

impl RootSpanBuilder for CustomRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let trace_id = request
            .headers()
            .get("trace-id")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        if !trace_id.is_empty() {
            tracing_actix_web::root_span!(request, trace_id = trace_id)
        } else {
            // Fallback to default behavior if header is missing
            tracing_actix_web::root_span!(request)
        }
    }

    fn on_request_end<B: MessageBody>(span: Span, outcome: &Result<ServiceResponse<B>, Error>) {
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}
