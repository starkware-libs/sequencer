use std::collections::HashSet;

use tracing::metadata::LevelFilter;
use tracing_subscriber::{reload, EnvFilter};

use crate::trace_util::{set_log_level, ReloadHandle};

#[test]
fn log_level_directive_updates() {
    let filter = EnvFilter::new("info");
    let (_layer, reload_handle): (reload::Layer<_, _>, ReloadHandle) = reload::Layer::new(filter);

    set_log_level(&reload_handle, "a", LevelFilter::DEBUG);
    set_log_level(&reload_handle, "b", LevelFilter::DEBUG);
    let directives = reload_handle.with_current(|f| f.to_string()).expect("handle should be valid");
    let directive_set: HashSet<&str> = directives.split(',').map(|s| s.trim()).collect();
    let expected: HashSet<&str> = ["info", "a=debug", "b=debug"].into_iter().collect();
    assert_eq!(directive_set, expected);
    set_log_level(&reload_handle, "a", LevelFilter::INFO);
    let directives = reload_handle.with_current(|f| f.to_string()).expect("handle should be valid");
    let directive_set: HashSet<&str> = directives.split(',').map(|s| s.trim()).collect();
    let expected: HashSet<&str> = ["info", "a=info", "b=debug"].into_iter().collect();
    assert_eq!(directive_set, expected);
}
