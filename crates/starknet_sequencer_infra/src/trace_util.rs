use std::sync::Once;

use tracing::metadata::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;
static INIT_TRACING: Once = Once::new();

pub fn configure_tracing() {
    INIT_TRACING.call_once(|| {
        let fmt_layer = fmt::layer().compact().with_target(true);
        let level_filter_layer =
            EnvFilter::builder().with_default_directive(DEFAULT_LEVEL.into()).from_env_lossy();

        tracing_subscriber::registry().with(fmt_layer).with(level_filter_layer).init();
    });
}
