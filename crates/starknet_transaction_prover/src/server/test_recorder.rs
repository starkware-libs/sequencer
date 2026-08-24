//! Test-only helper for sharing a Prometheus recorder across unit tests.
//!
//! `metrics-exporter-prometheus` installs into a single global recorder, so two tests that each
//! call `install_exporter` would race on it; this module installs once via a `OnceLock` and hands
//! the same handle to every test.

use std::sync::OnceLock;

use metrics_exporter_prometheus::PrometheusHandle;

use crate::server::metrics::{install_exporter, names};

static SHARED_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Returns the shared [`PrometheusHandle`], installing the recorder on the
/// first call. Safe to call from any test, including in parallel.
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

/// Prometheus sample line for the proving outcome counter at a given `outcome`
/// label. Callers baseline it before a request and assert the delta after,
/// because the recorder is process-global.
pub fn outcome_total_line(outcome: &str) -> String {
    format!("{}{{outcome=\"{}\"}}", names::PROVE_TRANSACTION_OUTCOME_TOTAL, outcome)
}

/// Prometheus `_count` line of the proving duration histogram for one `outcome`.
/// The histogram is labelled, so the label is required to pin a single series.
pub fn duration_count_line(outcome: &str) -> String {
    format!("{}_count{{outcome=\"{}\"}}", names::PROVE_TRANSACTION_DURATION_SECONDS, outcome)
}
