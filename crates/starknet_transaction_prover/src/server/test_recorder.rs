//! Test-only helper for sharing a Prometheus recorder across unit tests.
//!
//! `metrics-exporter-prometheus` installs into a single global recorder.
//! Two tests that each call `install_exporter` race on it: the second
//! install fails with "attempted to set a recorder after the metrics
//! system was already initialized". This module exposes a `OnceLock` that
//! installs exactly once and hands the same handle to every test, with
//! generic version/git_sha labels so no test depends on specific values.

use std::sync::OnceLock;

use metrics_exporter_prometheus::PrometheusHandle;

use crate::server::metrics::install_exporter;

static SHARED_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Returns the shared [`PrometheusHandle`], installing the recorder on the
/// first call. Safe to call from any test (including in parallel) — the
/// `OnceLock` synchronizes installation.
pub fn shared_handle() -> &'static PrometheusHandle {
    SHARED_HANDLE
        .get_or_init(|| install_exporter("0.0.0-test", "test-sha").expect("install test recorder"))
}
