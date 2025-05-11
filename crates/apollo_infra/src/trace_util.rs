use time::format_description::BorrowedFormatItem;
use time::macros::format_description;
use tokio::sync::OnceCell;
use tracing::info;
use tracing::metadata::LevelFilter;
use tracing_subscriber::fmt::format::{Compact, DefaultFields, Format};
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::fmt::Layer;
use tracing_subscriber::layer::Layered;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, reload, EnvFilter, Registry};

const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;
// Define a OnceCell to ensure the configuration is initialized only once
static TRACING_INITIALIZED: OnceCell<()> = OnceCell::const_new();

type TracingLayer<'a> = Layered<
    Layer<Registry, DefaultFields, Format<Compact, UtcTime<&'a [BorrowedFormatItem<'a>]>>>,
    Registry,
>;

pub static TRACING_LEVEL_HANDLE: OnceCell<reload::Handle<EnvFilter, TracingLayer<'_>>> =
    OnceCell::const_new();

pub static PID: std::sync::LazyLock<u32> = std::sync::LazyLock::new(std::process::id);

pub async fn configure_tracing() {
    TRACING_INITIALIZED
        .get_or_init(|| async {
            // Use default time formatting with subsecond precision limited to three digits.
            let time_format = format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
            );
            let timer = UtcTime::new(time_format);

            let fmt_layer = fmt::layer()
                .compact()
                .with_timer(timer)
                .with_target(false) // No module name.
                // Instead, file name and line number.
                .with_file(true)
                .with_line_number(true);

            let (level_filter_layer, reload_handle) =
                reload::Layer::new(get_filter_layer(DEFAULT_LEVEL, true));

            // This sets a single subscriber to all of the threads. We may want to implement
            // different subscriber for some threads and use set_global_default instead
            // of init.
            tracing_subscriber::registry().with(fmt_layer).with(level_filter_layer).init();
            tracing::info!("Tracing has been successfully initialized.");

            TRACING_LEVEL_HANDLE.set(reload_handle).expect("Failed to set TRACING_LEVEL_HANDLE");
        })
        .await;
}

fn get_filter_layer(level: LevelFilter, use_env_var: bool) -> EnvFilter {
    let builder = EnvFilter::builder().with_default_directive(level.into());
    let filter = if use_env_var { builder.from_env_lossy() } else { builder.parse_lossy("") };

    filter
        .add_directive("alloy_provider=info".parse().unwrap())
        .add_directive("alloy_transport_http=info".parse().unwrap())
        .add_directive("alloy_rpc_client=info".parse().unwrap())
        .add_directive("hyper=info".parse().unwrap())
        .add_directive("hyper_util=info".parse().unwrap())
        .add_directive("libp2p=info".parse().unwrap())
        .add_directive("libp2p-gossipsub=info".parse().unwrap())
        .add_directive("multistream_select=info".parse().unwrap())
        .add_directive("netlink_proto=info".parse().unwrap())
        .add_directive("yamux=info".parse().unwrap())
}

pub async fn change_tracing_level(level: LevelFilter) {
    if let Some(handle) = TRACING_LEVEL_HANDLE.get() {
        info!("*** Changing tracing level to: {:?}", level);
        handle.reload(get_filter_layer(level, false)).expect("Failed to reload filter layer");
    } else {
        tracing::warn!("Tracing level handle is not initialized.");
    }
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
