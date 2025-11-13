use tracing::metadata::LevelFilter;

use crate::trace_util::{configure_tracing, set_log_level};

#[tokio::test]
async fn log_level_directive_updates() {
    let reload_handle = configure_tracing().await;

    set_log_level("a", LevelFilter::DEBUG).await;
    set_log_level("b", LevelFilter::DEBUG).await;
    let directives = reload_handle.with_current(|f| f.to_string()).expect("handle should be valid");

    assert!(directives.contains("a=debug"));
    assert!(directives.contains("b=debug"));

    set_log_level("a", LevelFilter::INFO).await;
    let directives = reload_handle.with_current(|f| f.to_string()).expect("handle should be valid");
    assert!(directives.contains("a=info"));
}
