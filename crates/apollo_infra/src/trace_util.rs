use std::io::stdout;
use std::str::FromStr;

use serde_json::json;
use time::macros::format_description;
use tokio::sync::OnceCell;
use tracing::field::{Field, Visit};
use tracing::metadata::LevelFilter;
use tracing::{warn, Event, Subscriber};
use tracing_subscriber::filter::Directive;
// Remove the problematic FmtContext import line
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{fmt, reload, EnvFilter};

// Crates we always keep at INFO regardless of operator-supplied spec.
const QUIET_LIBS: &[&str] = &[
    "alloy_provider",
    "alloy_transport_http",
    "alloy_rpc_client",
    "futures-util",
    "hickory-proto",
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

pub static PID: std::sync::LazyLock<u32> = std::sync::LazyLock::new(std::process::id);

pub async fn configure_tracing() -> ReloadHandle {
    let reload_handle = TRACING_INITIALIZED
        .get_or_init(|| async {
            // Use default time formatting with sub-second precision limited to three digits.
            let time_format = format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
            );
            let timer = UtcTime::new(time_format);

            let fmt_layer = fmt::layer()
                .json()
                .with_timer(timer)
                .with_target(false) // No module name.
                // Instead, file name and line number.
                .with_file(true)
                .with_line_number(true)
                .flatten_event(true);

            let level_filter_layer = QUIET_LIBS.iter().fold(
                EnvFilter::builder().with_default_directive(DEFAULT_LEVEL.into()).from_env_lossy(),
                |layer, lib| layer.add_directive(format!("{lib}=info").parse().unwrap()),
            );

            // Wrap the EnvFilter in a reloadable layer so that it can be updated at runtime.
            let (filtered_layer, reload_handle) = reload::Layer::new(level_filter_layer);

            // This sets a single subscriber to all of the threads. We may want to implement
            // different subscriber for some threads and use set_global_default instead
            // of init.
            tracing_subscriber::registry().with(filtered_layer).with(fmt_layer).init();
            tracing::info!("Tracing has been successfully initialized.");

            reload_handle
        })
        .await;

    reload_handle.clone()
}

pub async fn configure_tracing_new() -> ReloadHandle {
    let reload_handle = TRACING_INITIALIZED
        .get_or_init(|| async {
            // Use default time formatting with sub-second precision limited to three digits.

            let fmt_layer = fmt::layer()
                // .json()
                // .with_timer(timer)
                // .with_target(false) // No module name.
                // // Instead, file name and line number.
                // .with_file(true)
                // .with_line_number(true)
                // .flatten_event(true);
                .event_format(CustomJsonEventFormatter)
                .with_writer(stdout);

            let level_filter_layer = QUIET_LIBS.iter().fold(
                EnvFilter::builder().with_default_directive(DEFAULT_LEVEL.into()).from_env_lossy(),
                |layer, lib| layer.add_directive(format!("{lib}=info").parse().unwrap()),
            );

            // Wrap the EnvFilter in a reloadable layer so that it can be updated at runtime.
            let (filtered_layer, reload_handle) = reload::Layer::new(level_filter_layer);

            // This sets a single subscriber to all of the threads. We may want to implement
            // different subscriber for some threads and use set_global_default instead
            // of init.
            tracing_subscriber::registry().with(filtered_layer).with(fmt_layer).init();
            tracing::info!("Tracing has been successfully initialized.");

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

/// A custom visitor that collects fields into a JSON map, renaming specific keys.
struct FieldRenamingVisitor(serde_json::Map<String, serde_json::Value>);

impl FieldRenamingVisitor {
    fn new() -> Self {
        FieldRenamingVisitor(serde_json::Map::new())
    }

    fn record_value<T: serde::Serialize + std::fmt::Debug>(&mut self, field: &Field, value: T) {
        self.0.insert(field.name().to_string(), json!(value));
    }
}

// We implement `Visit` to intercept how fields are recorded.
impl Visit for FieldRenamingVisitor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        // Example: If we see a field named "PID" (used in your macros), rename it to "process_id"
        if field.name() == "PID" {
            self.0.insert("process_id".to_string(), json!(value));
        } else {
            self.record_value(field, value);
        }
    }

    // Implement other specific types as needed, or fall back to debug.
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_string(), json!(format!("{:?}", value)));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), json!(value));
    }

    // ... Implement record_bool, record_f64, record_u64, etc. for a complete implementation.
}

/// A custom `FormatEvent` implementation that uses our `FieldRenamingVisitor`
/// to build the final JSON output with standard fields included.
struct CustomJsonEventFormatter;

impl<S, N> FormatEvent<S, N> for CustomJsonEventFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let mut visitor = FieldRenamingVisitor::new();
        event.record(&mut visitor);

        let metadata = event.metadata();
        let file_name = event.metadata().file().unwrap_or("unknown");
        let line_number = event.metadata().line().unwrap_or(0);

        let final_json = json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            // FIX: Use .to_string() or format!() to get the string representation
            "level": metadata.level().to_string(),
            "target": metadata.target(),
            "file": file_name,
            "line": line_number,
            "fields": visitor.0,
        });

        writeln!(writer, "{}", final_json.to_string())
    }
}
