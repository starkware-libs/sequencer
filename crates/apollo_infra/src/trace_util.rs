use time::macros::format_description;
use tokio::sync::OnceCell;
use tracing::metadata::LevelFilter;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::prelude::*;
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
type ReloadHandle = reload::Handle<EnvFilter, tracing_subscriber::Registry>;

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
