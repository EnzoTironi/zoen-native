//! Logs and traces. Logs go to stdout (pretty, or JSON with `LOG_FORMAT=json`), filtered by
//! `RUST_LOG`. When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, this crate's own spans and log
//! events also go out over OTLP/HTTP (traces and logs), sampled per the standard
//! `OTEL_TRACES_SAMPLER` variables, with W3C trace context carried across the NATS bus.
//!
//! What may leave the process (ADR 0021): span names, pseudonymized identifiers
//! (`pseudonym::pseudo`), partitions, sequence numbers, counts, outcomes and routes. Never a
//! raw identity, device, Space, handle, invite code, IP address, URL or any envelope bytes.
//! Only spans and events whose target is `zoen_relay` are exported, so a dependency that logs
//! a query, a header or a URL can never reach the collector.

use opentelemetry::{global, trace::TracerProvider as _, KeyValue};
use opentelemetry_otlp::{LogExporter, SpanExporter};
use opentelemetry_sdk::{
    logs::SdkLoggerProvider, propagation::TraceContextPropagator, trace::SdkTracerProvider,
    Resource,
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    filter::{filter_fn, Targets},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    Layer,
};

/// Flushes and stops the exporters when dropped.
pub struct Telemetry {
    traces: Option<SdkTracerProvider>,
    logs: Option<SdkLoggerProvider>,
}

/// Call before starting the async runtime: the OTLP exporters run on their own threads.
pub fn init(service: &'static str) -> anyhow::Result<Telemetry> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,sqlx=warn,tower_http=warn"));
    let stdout = if std::env::var("LOG_FORMAT").as_deref() == Ok("json") {
        tracing_subscriber::fmt::layer()
            .json()
            .with_current_span(false)
            .with_filter(env_filter)
            .boxed()
    } else {
        // Colour only for a person at a terminal: escapes in a file or a log pipeline break
        // every reader that searches the text.
        use std::io::IsTerminal;
        let ansi = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        tracing_subscriber::fmt::layer()
            .with_ansi(ansi)
            .with_filter(env_filter)
            .boxed()
    };

    let exporting = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok_and(|v| !v.is_empty());
    let (traces, logs) = if exporting {
        let resource = Resource::builder()
            .with_service_name(service)
            .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
            .build();
        let traces = SdkTracerProvider::builder()
            .with_resource(resource.clone())
            .with_batch_exporter(SpanExporter::builder().with_http().build()?)
            .build();
        let logs = SdkLoggerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(LogExporter::builder().with_http().build()?)
            .build();
        global::set_text_map_propagator(TraceContextPropagator::new());
        (Some(traces), Some(logs))
    } else {
        (None, None)
    };

    // Spans become traces and events become log records (carrying the trace they happened
    // in), so nothing is exported twice.
    let ours = || Targets::new().with_target("zoen_relay", LevelFilter::INFO);
    let span_layer = traces.as_ref().map(|p| {
        tracing_opentelemetry::layer()
            .with_tracer(p.tracer(service))
            .with_threads(false)
            .with_tracked_inactivity(false)
            .with_filter(ours())
            .with_filter(filter_fn(|m| m.is_span()))
    });
    let log_layer = logs.as_ref().map(|p| {
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(p)
            .with_filter(ours())
            .with_filter(filter_fn(|m| m.is_event()))
    });
    tracing_subscriber::registry()
        .with(stdout)
        .with(span_layer)
        .with(log_layer)
        .try_init()?;
    if exporting {
        tracing::info!("exporting traces and logs over OTLP");
    }
    Ok(Telemetry { traces, logs })
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        if let Some(p) = self.traces.take() {
            let _ = p.shutdown();
        }
        if let Some(p) = self.logs.take() {
            let _ = p.shutdown();
        }
    }
}

/// The current span's trace context as W3C headers (`traceparent`, `tracestate`), to send
/// along with a frame on the bus. Empty when nothing is exported or the span isn't sampled.
pub fn current_context_headers() -> Vec<(String, String)> {
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    let cx = tracing::Span::current().context();
    let mut carrier = std::collections::HashMap::new();
    global::get_text_map_propagator(|p| p.inject_context(&cx, &mut carrier));
    carrier.into_iter().collect()
}

/// Makes `span` a child of the remote context in `headers` (see `current_context_headers`).
pub fn adopt_remote_parent(
    span: &tracing::Span,
    headers: &dyn opentelemetry::propagation::Extractor,
) {
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    let parent = global::get_text_map_propagator(|p| p.extract(headers));
    let _ = span.set_parent(parent);
}
