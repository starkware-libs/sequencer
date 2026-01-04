use std::str::FromStr;

use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, TracerProvider as SdkTracerProvider};
use serde_json::Value;
use time::macros::format_description;
use tokio::sync::OnceCell;
use tracing::metadata::LevelFilter;
use tracing::{warn, Subscriber};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{fmt, reload, EnvFilter, Registry};

use crate::otel_context::get_current_trace_id;

/// Custom JSON formatter that includes OpenTelemetry trace_id in log output.
struct TraceIdJsonFormat<F>(F);

impl<S, N, F> FormatEvent<S, N> for TraceIdJsonFormat<F>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
    F: FormatEvent<S, N>,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let mut buf = String::new();
        self.0.format_event(ctx, Writer::new(&mut buf), event)?;

        if let (Some(tid), Ok(mut json)) =
            (get_current_trace_id(), serde_json::from_str::<Value>(buf.trim()))
        {
            if let Some(obj) = json.as_object_mut() {
                obj.insert("trace_id".into(), Value::String(tid));
            }
            if let Ok(output) = serde_json::to_string(&json) {
                return writeln!(writer, "{}", output);
            }
        }
        write!(writer, "{}", buf)
    }
}

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

type OtelRegistry = tracing_subscriber::layer::Layered<
    OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::Tracer>,
    Registry,
>;
pub(crate) type ReloadHandle = reload::Handle<EnvFilter, OtelRegistry>;

// Define a OnceCell to ensure the configuration is initialized only once
static TRACING_INITIALIZED: OnceCell<ReloadHandle> = OnceCell::const_new();

// Keep the tracer provider alive for the lifetime of the program
static TRACER_PROVIDER: std::sync::OnceLock<SdkTracerProvider> = std::sync::OnceLock::new();

pub static PID: std::sync::LazyLock<u32> = std::sync::LazyLock::new(std::process::id);

/// Create an OpenTelemetry tracer provider for cross-component tracing.
fn create_tracer_provider() -> SdkTracerProvider {
    SdkTracerProvider::builder()
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .build()
}

pub async fn configure_tracing() -> ReloadHandle {
    let reload_handle = TRACING_INITIALIZED
        .get_or_init(|| async {
            // Use default time formatting with sub-second precision limited to three digits.
            let time_format = format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
            );
            let timer = UtcTime::new(time_format);

            // Create a JSON formatter layer with custom trace_id injection
            let json_format = TraceIdJsonFormat(
                fmt::format::json()
                    .with_timer(timer)
                    .with_target(false) // No module name.
                    // Instead, file name and line number.
                    .with_file(true)
                    .with_line_number(true)
                    .flatten_event(true),
            );

            let fmt_layer =
                fmt::layer().event_format(json_format).fmt_fields(fmt::format::JsonFields::new());

            let level_filter_layer = QUIET_LIBS.iter().fold(
                EnvFilter::builder().with_default_directive(DEFAULT_LEVEL.into()).from_env_lossy(),
                |layer, lib| layer.add_directive(format!("{lib}=info").parse().unwrap()),
            );

            // Create OpenTelemetry tracer provider and store it in a static
            let provider = TRACER_PROVIDER.get_or_init(create_tracer_provider);
            // The name is just metadata for opentelemetry, not used for tracing
            let tracer = provider.tracer("apollo");
            let otel_layer = OpenTelemetryLayer::new(tracer);

            // Wrap the EnvFilter in a reloadable layer so that it can be updated at runtime.
            let (filtered_layer, reload_handle) = reload::Layer::new(level_filter_layer);

            // This sets a single subscriber to all of the threads. We may want to implement
            // different subscriber for some threads and use set_global_default instead
            // of init.
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(filtered_layer)
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
