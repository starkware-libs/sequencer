//! Test-only helper for sharing a Prometheus recorder across unit tests.
//!
//! `metrics-exporter-prometheus` installs into a single global recorder, so two tests that each
//! call `install_exporter` would race on it. This module installs the recorder once through a
//! `OnceLock` and hands the same handle to every test.

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

/// Parses a named sample, returning `0.0` when the series is absent.
pub fn metric_value(scrape: &str, needle: &str) -> f64 {
    scrape
        .lines()
        .filter(|line| !line.starts_with('#'))
        .find(|line| line.starts_with(needle))
        .and_then(|line| line.rsplit_once(' '))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0.0)
}

/// Prometheus sample line for a proving outcome counter.
pub fn outcome_total_line(outcome: &str) -> String {
    format!("{}{{outcome=\"{}\"}}", names::PROVE_TRANSACTION_OUTCOME_TOTAL, outcome)
}

/// Prometheus `_count` line for a proving duration histogram outcome.
pub fn duration_count_line(outcome: &str) -> String {
    format!("{}_count{{outcome=\"{}\"}}", names::PROVE_TRANSACTION_DURATION_SECONDS, outcome)
}
