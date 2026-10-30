//! JSON log lines in the shape Cloud Logging understands.
//!
//! Cloud Run parses each stdout line as a `LogEntry`: it reads `severity`,
//! `message` and `logging.googleapis.com/trace` from the top level. The
//! default `tracing_subscriber` JSON format writes `level` and nests the
//! message, so every line lands with no severity and is not grouped under
//! its request in the Logs Explorer.

use std::env;
use std::fmt;

use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::fmt::format::{JsonFields, Writer};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormattedFields};
use tracing_subscriber::registry::LookupSpan;

static PROJECT_ENV: &str = "GOOGLE_CLOUD_PROJECT";
static TRACE_KEY: &str = "logging.googleapis.com/trace";
/// Span field holding the Cloud Trace id (see `api_core::tracing_utils`).
static TRACE_ID_FIELD: &str = "trace_id";

/// Formats events as single-line Cloud Logging JSON.
#[derive(Debug, Clone, Default)]
pub struct CloudLoggingFormat {
    project_id: Option<String>,
}

impl CloudLoggingFormat {
    /// Reads the project used to link entries to Cloud Trace from
    /// `GOOGLE_CLOUD_PROJECT`; without it, entries carry no trace link.
    pub fn from_env() -> Self {
        Self {
            project_id: env::var(PROJECT_ENV).ok().filter(|p| !p.trim().is_empty()),
        }
    }
}

/// A `fmt` layer that writes [`CloudLoggingFormat`] lines.
pub fn layer<S>() -> tracing_subscriber::fmt::Layer<S, JsonFields, CloudLoggingFormat>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .fmt_fields(JsonFields::new())
        .event_format(CloudLoggingFormat::from_env())
}

fn severity(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARNING",
        Level::INFO => "INFO",
        _ => "DEBUG",
    }
}

#[derive(Default)]
struct FieldMap(Map<String, Value>);

impl Visit for FieldMap {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().into(), value.into());
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().into(), value.into());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().into(), value.into());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().into(), value.into());
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().into(), value.into());
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.0.insert(field.name().into(), value.to_string().into());
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0
            .insert(field.name().into(), format!("{value:?}").into());
    }
}

impl<S> FormatEvent<S, JsonFields> for CloudLoggingFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, JsonFields>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let mut fields = FieldMap::default();
        event.record(&mut fields);
        let mut entry = fields.0;
        let message = entry.remove("message").unwrap_or(Value::Null);

        // Root first, so inner spans win on a name clash.
        let span: Map<String, Value> = ctx
            .event_scope()
            .into_iter()
            .flat_map(|scope| scope.from_root())
            .filter_map(|s| {
                s.extensions()
                    .get::<FormattedFields<JsonFields>>()
                    .and_then(|f| serde_json::from_str::<Map<String, Value>>(f).ok())
            })
            .flatten()
            .collect();

        let trace = self.project_id.as_ref().and_then(|project| {
            span.get(TRACE_ID_FIELD)
                .and_then(Value::as_str)
                .map(|id| format!("projects/{project}/traces/{id}"))
        });

        entry.insert("severity".into(), severity(meta.level()).into());
        entry.insert("message".into(), message);
        entry.insert("target".into(), meta.target().into());
        if let Some(trace) = trace {
            entry.insert(TRACE_KEY.into(), trace.into());
        }
        if !span.is_empty() {
            entry.insert("span".into(), Value::Object(span));
        }

        let line = serde_json::to_string(&entry).map_err(|_| fmt::Error)?;
        writeln!(writer, "{line}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Buf {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| io::Error::other("poisoned"))?
                .extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn capture(project: Option<&str>, emit: impl FnOnce()) -> Vec<Value> {
        let buf = Buf::default();
        let sink = buf.clone();
        let format = CloudLoggingFormat {
            project_id: project.map(str::to_string),
        };
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .fmt_fields(JsonFields::new())
                .event_format(format)
                .with_writer(move || sink.clone()),
        );
        tracing::subscriber::with_default(subscriber, emit);
        let bytes = buf.0.lock().map(|b| b.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes)
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    #[test]
    fn warn_maps_to_warning_severity_with_top_level_message() {
        let lines = capture(None, || tracing::warn!(code = 400, "rejected"));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["severity"], "WARNING");
        assert_eq!(lines[0]["message"], "rejected");
        assert_eq!(lines[0]["code"], 400);
    }

    #[test]
    fn links_trace_from_span_when_project_known() {
        let lines = capture(Some("proj"), || {
            let span = tracing::info_span!("HTTP request", trace_id = "abc123");
            let _g = span.enter();
            tracing::error!("boom");
        });
        assert_eq!(lines[0]["severity"], "ERROR");
        assert_eq!(lines[0][TRACE_KEY], "projects/proj/traces/abc123");
        assert_eq!(lines[0]["span"]["trace_id"], "abc123");
    }

    #[test]
    fn omits_trace_without_project() {
        let lines = capture(None, || {
            let span = tracing::info_span!("HTTP request", trace_id = "abc123");
            let _g = span.enter();
            tracing::info!("hi");
        });
        assert!(lines[0].get(TRACE_KEY).is_none());
    }
}
