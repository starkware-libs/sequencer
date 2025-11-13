use tracing::metadata::LevelFilter;
use tracing_subscriber::{reload, EnvFilter};

use crate::trace_util::{get_log_directives, set_log_level, ReloadHandle};

#[test]
fn log_level_directive_updates() {
    let filter = EnvFilter::new("info");
    let (_layer, reload_handle): (reload::Layer<_, _>, ReloadHandle) = reload::Layer::new(filter);

    set_log_level(&reload_handle, "a", LevelFilter::DEBUG);
    set_log_level(&reload_handle, "b", LevelFilter::DEBUG);
    let directives = get_log_directives(&reload_handle);
    let expected = ["info", "a=debug", "b=debug"].into_iter().map(String::from).collect();
    assert_eq!(directives, expected);
    set_log_level(&reload_handle, "a", LevelFilter::INFO);
    let directives = get_log_directives(&reload_handle);
    let expected = ["info", "a=info", "b=debug"].into_iter().map(String::from).collect();
    assert_eq!(directives, expected);
}
