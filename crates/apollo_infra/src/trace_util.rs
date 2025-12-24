use std::io::{self, Write as IoWrite};
use std::str::FromStr;

use hyper::header::{HeaderName, HeaderValue};
use opentelemetry::global::set_text_map_propagator;
use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator};
use opentelemetry::trace::{TraceContextExt, TracerProvider};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::TracerProvider as SdkTracerProvider;
use time::macros::format_description;
use tokio::sync::OnceCell;
use tracing::metadata::LevelFilter;
use tracing::warn;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::fmt::{self, MakeWriter};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{reload, EnvFilter};

// Crates we always keep at INFO regardless of operator-supplied spec.
const QUIET_LIBS: &[&str] = &[
    "alloy_provider",
    "alloy_transport_http",
    "alloy_rpc_client",
    "futures-util",
    "hickory-proto",
    "hickory-resolver",
    "hyper",
    "hyper_util",
    "h2",
    "libp2p",
    "libp2p-gossipsub",
    "multistream_select",
    "netlink_proto",
    "reqwest",
    "yamux",
];

const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;
pub(crate) type ReloadHandle = reload::Handle<EnvFilter, tracing_subscriber::Registry>;

// Define a OnceCell to ensure the configuration is initialized only once
static TRACING_INITIALIZED: OnceCell<ReloadHandle> = OnceCell::const_new();

/// A writer wrapper that injects OpenTelemetry trace context (trace_id, span_id) into JSON log
/// lines. This preserves the original JSON format while adding trace correlation fields.
pub struct TraceContextWriter<W> {
    inner: W,
    buffer: Vec<u8>,
}

impl<W: IoWrite> TraceContextWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, buffer: Vec::with_capacity(1024) }
    }

    fn inject_trace_context(&mut self) -> io::Result<()> {
        use serde_json::{Map, Value};
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        // Parse buffer as JSON and inject trace context fields.
        if let Ok(Value::Object(mut map)) = serde_json::from_slice(&self.buffer) {
            let otel_context = tracing::Span::current().context();
            let otel_span = otel_context.span();
            let span_context = otel_span.span_context();

            if span_context.is_valid() {
                // Build new map with trace context first, then original fields.
                let mut new_map = Map::with_capacity(map.len() + 2);
                new_map.insert("trace_id".into(), span_context.trace_id().to_string().into());
                new_map.insert("span_id".into(), span_context.span_id().to_string().into());
                new_map.extend(map);
                map = new_map;
            }

            serde_json::to_writer(&mut self.inner, &map)?;
            self.inner.write_all(b"\n")?;
        } else {
            // Not valid JSON, write as-is.
            self.inner.write_all(&self.buffer)?;
        }

        self.buffer.clear();
        Ok(())
    }
}

impl<W: IoWrite> IoWrite for TraceContextWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Buffer the data until we see a newline.
        self.buffer.extend_from_slice(buf);

        // Check if we have a complete line (ends with newline).
        if buf.ends_with(b"\n") {
            self.inject_trace_context()?;
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            self.inject_trace_context()?;
        }
        self.inner.flush()
    }
}

/// A MakeWriter implementation that wraps writers with trace context injection.
#[derive(Clone)]
pub struct TraceContextMakeWriter<M> {
    inner: M,
}

impl<M> TraceContextMakeWriter<M> {
    pub fn new(inner: M) -> Self {
        Self { inner }
    }
}

impl<'a, M> MakeWriter<'a> for TraceContextMakeWriter<M>
where
    M: MakeWriter<'a>,
{
    type Writer = TraceContextWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        TraceContextWriter::new(self.inner.make_writer())
    }
}

pub static PID: std::sync::LazyLock<u32> = std::sync::LazyLock::new(std::process::id);

pub async fn configure_tracing() -> ReloadHandle {
    let reload_handle = TRACING_INITIALIZED
        .get_or_init(|| async {
            // Set up W3C trace context propagator for cross-service trace propagation.
            set_text_map_propagator(TraceContextPropagator::new());

            // Create an OpenTelemetry tracer provider.
            // This is a simple in-memory provider; traces are linked via context propagation
            // but not exported to an external collector.
            let tracer_provider = SdkTracerProvider::builder().build();
            let tracer = tracer_provider.tracer("apollo");
            let otel_layer = OpenTelemetryLayer::new(tracer);

            // Use default time formatting with sub-second precision limited to three digits.
            let time_format = format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
            );
            let timer = UtcTime::new(time_format);

            // Use the original JSON formatter wrapped with trace context injection.
            // This preserves all original formatting while adding trace_id and span_id.
            let fmt_layer = fmt::layer()
                .json()
                .with_timer(timer)
                .with_target(false) // No module name.
                .with_file(true)
                .with_line_number(true)
                .flatten_event(true)
                .with_writer(TraceContextMakeWriter::new(std::io::stdout));

            let level_filter_layer = QUIET_LIBS.iter().fold(
                EnvFilter::builder().with_default_directive(DEFAULT_LEVEL.into()).from_env_lossy(),
                |layer, lib| layer.add_directive(format!("{lib}=info").parse().unwrap()),
            );

            // Wrap the EnvFilter in a reloadable layer so that it can be updated at runtime.
            let (filtered_layer, reload_handle) = reload::Layer::new(level_filter_layer);

            // This sets a single subscriber to all of the threads. We may want to implement
            // different subscriber for some threads and use set_global_default instead
            // of init.
            tracing_subscriber::registry()
                .with(filtered_layer)
                .with(otel_layer)
                .with(fmt_layer)
                .init();
            tracing::info!("Tracing has been successfully initialized with OpenTelemetry support.");

            reload_handle
        })
        .await;

    reload_handle.clone()
}

#[macro_export]
macro_rules! infra_event {
    ($($arg:tt)*) => {{
        tracing::event!(PID = *$crate::trace_util::PID, $($arg)*);
    }};
}

#[macro_export]
macro_rules! infra_trace {
    ($($arg:tt)*) => {{
        tracing::trace!(PID = *$crate::trace_util::PID, $($arg)*);
    }};
}

#[macro_export]
macro_rules! infra_debug {
    ($($arg:tt)*) => {{
        tracing::debug!(PID = *$crate::trace_util::PID, $($arg)*);
    }};
}

#[macro_export]
macro_rules! infra_info {
    ($($arg:tt)*) => {{
        tracing::info!(PID = *$crate::trace_util::PID, $($arg)*);
    }};
}

#[macro_export]
macro_rules! infra_warn {
    ($($arg:tt)*) => {{
        tracing::warn!(PID = *$crate::trace_util::PID, $($arg)*);
    }};
}

#[macro_export]
macro_rules! infra_error {
    ($($arg:tt)*) => {{
        tracing::error!(PID = *$crate::trace_util::PID, $($arg)*);
    }};
}

pub fn set_log_level(handle: &ReloadHandle, crate_name: &str, level: LevelFilter) {
    if let Ok(directive) = Directive::from_str(&format!("{crate_name}={level}")) {
        let _ = handle.modify(|filter| {
            *filter = std::mem::take(filter).add_directive(directive);
        });
    } else {
        warn!("{crate_name}: ignored invalid log-level directive");
    }
}

pub fn get_log_directives(handle: &ReloadHandle) -> Result<String, reload::Error> {
    handle.with_current(|f| f.to_string())
}

/// Extracts trace context headers from the given OpenTelemetry context for cross-service
/// propagation.
pub fn extract_trace_headers(context: &opentelemetry::Context) -> Vec<(HeaderName, HeaderValue)> {
    let mut header_injector = HeaderInjector::default();
    let propagator = TraceContextPropagator::new();
    propagator.inject_context(context, &mut header_injector);

    header_injector.headers
}

/// Helper struct for injecting OpenTelemetry trace context into HTTP headers.
#[derive(Default)]
struct HeaderInjector {
    headers: Vec<(HeaderName, HeaderValue)>,
}

impl Injector for HeaderInjector {
    fn set(&mut self, key: &str, value: String) {
        let header_name = HeaderName::try_from(key)
            .expect("OpenTelemetry header names need to be valid HTTP headers");
        let header_value = HeaderValue::try_from(&value)
            .expect("OpenTelemetry header values need to be valid HTTP header values");
        self.headers.push((header_name, header_value));
    }
}

/// Extracts trace context from HTTP headers for cross-service propagation.
pub fn extract_context_from_headers(headers: &hyper::HeaderMap) -> opentelemetry::Context {
    let header_extractor = HeaderExtractor::new(headers);
    let propagator = TraceContextPropagator::new();
    propagator.extract(&header_extractor)
}

/// Helper struct for extracting OpenTelemetry trace context from HTTP headers.
struct HeaderExtractor<'a> {
    headers: &'a hyper::HeaderMap,
}

impl<'a> HeaderExtractor<'a> {
    fn new(headers: &'a hyper::HeaderMap) -> Self {
        Self { headers }
    }
}

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(|k| k.as_str()).collect()
    }
}
