use std::io;

use time::macros::format_description;
use tokio::sync::OnceCell;
use tracing::metadata::LevelFilter;
use tracing_subscriber::filter::{FilterExt, FilterFn};
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;
// Define a OnceCell to ensure the configuration is initialized only once
static TRACING_INITIALIZED: OnceCell<()> = OnceCell::const_new();

pub static PID: std::sync::LazyLock<u32> = std::sync::LazyLock::new(std::process::id);

pub async fn configure_tracing() {
    TRACING_INITIALIZED
        .get_or_init(|| async {
            // Use default time formatting with subsecond precision limited to three digits.
            let time_format = format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
            );

            // Create formatter for stdout (info, warn, debug, trace)
            let stdout_fmt = fmt::layer()
                .compact()
                .with_timer(UtcTime::new(time_format.clone()))
                .with_target(false)
                .with_file(true)
                .with_line_number(true)
                .with_ansi(false)
                .with_level(true) // Ensure level is always shown prominently for GCP logging
                .with_writer(io::stdout);

            // Create formatter for stderr (error only)
            let stderr_fmt = fmt::layer()
                .compact()
                .with_timer(UtcTime::new(time_format))
                .with_target(false)
                .with_file(true)
                .with_line_number(true)
                .with_ansi(false)
                .with_level(true) // Ensure level is always shown prominently for GCP logging
                .with_writer(io::stderr);

            // Create filter for stdout (excludes ERROR level)
            let stdout_filter = EnvFilter::builder()
                .with_default_directive(DEFAULT_LEVEL.into())
                .from_env_lossy()
                .add_directive("alloy_provider=info".parse().unwrap())
                .add_directive("alloy_transport_http=info".parse().unwrap())
                .add_directive("alloy_rpc_client=info".parse().unwrap())
                .add_directive("futures-util=info".parse().unwrap())
                .add_directive("hyper=info".parse().unwrap())
                .add_directive("hyper_util=info".parse().unwrap())
                .add_directive("h2=info".parse().unwrap())
                .add_directive("libp2p=info".parse().unwrap())
                .add_directive("libp2p-gossipsub=info".parse().unwrap())
                .add_directive("multistream_select=info".parse().unwrap())
                .add_directive("netlink_proto=info".parse().unwrap())
                .add_directive("reqwest=info".parse().unwrap())
                .add_directive("yamux=info".parse().unwrap())
                .and(FilterFn::new(|metadata| metadata.level() != &tracing::Level::ERROR));

            // Create filter for stderr (only ERROR level)
            let stderr_filter =
                FilterFn::new(|metadata| metadata.level() == &tracing::Level::ERROR);

            // This sets a single subscriber to all of the threads. We may want to implement
            // different subscriber for some threads and use set_global_default instead
            // of init.
            tracing_subscriber::registry()
                .with(stdout_fmt.with_filter(stdout_filter))
                .with(stderr_fmt.with_filter(stderr_filter))
                .init();

            tracing::info!("Tracing has been successfully initialized.");
        })
        .await;
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
