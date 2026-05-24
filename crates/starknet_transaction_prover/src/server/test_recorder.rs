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

/// Parses the value of a single sample from a rendered Prometheus scrape.
///
/// `needle` must be the metric name plus whatever labels uniquely identify
/// the series (e.g. `prover_..._outcome_total{outcome="success"}`). Returns
/// `0.0` when the series is absent, so callers can take a baseline before an
/// action and assert the delta afterward — the recorder is process-global, so
/// other tests may have already moved the absolute value. `# HELP`/`# TYPE`
/// comment lines are skipped.
pub fn metric_value(scrape: &str, needle: &str) -> f64 {
    scrape
        .lines()
        .filter(|line| !line.starts_with('#'))
        .find(|line| line.starts_with(needle))
        .and_then(|line| line.rsplit_once(' '))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0.0)
}
