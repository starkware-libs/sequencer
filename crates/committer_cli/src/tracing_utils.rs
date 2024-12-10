use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::{filter, fmt, reload, Registry};

// TODO(Amos, 1/8/2024) Move all tracing instantiations in the Sequencer repo to a common location.
pub fn configure_tracing() -> Handle<LevelFilter, Registry> {
    // Create a handle to the global filter to allow setting log level at runtime.
    let (global_filter, global_filter_handle) = reload::Layer::new(filter::LevelFilter::INFO);
    let layer = fmt::Layer::default()
        .with_ansi(false)
        .with_target(false)
        .with_file(true)
        .with_line_number(true);
    tracing_subscriber::registry().with(global_filter).with(layer).init();
    global_filter_handle
}
