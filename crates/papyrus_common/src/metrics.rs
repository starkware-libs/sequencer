use std::sync::OnceLock;

// TODO(Shahak): consider making this value non static and add a way to change this while the app is
// running. e.g via a monitoring endpoint.
/// Global variable set by the main config to enable collecting profiling metrics.
pub static COLLECT_PROFILING_METRICS: OnceLock<bool> = OnceLock::new();
