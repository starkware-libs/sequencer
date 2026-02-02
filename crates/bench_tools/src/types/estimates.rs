use serde::{Deserialize, Serialize};

pub(crate) const NS_PER_MS: f64 = 1_000_000.0;

/// Entry format for github-action-benchmark's "customSmallerIsBetter" tool.
/// See: <https://github.com/benchmark-action/github-action-benchmark>
#[derive(Debug, Serialize, PartialEq)]
pub struct GithubBenchmarkEntry {
    pub name: String,
    pub unit: String,
    pub value: f64,
}

impl GithubBenchmarkEntry {
    /// Creates a GithubBenchmarkEntry from Criterion estimates.
    /// Converts nanoseconds to milliseconds.
    pub fn from_estimates(name: &str, estimates: &Estimates) -> Self {
        Self {
            name: name.to_string(),
            unit: "ms".to_string(),
            value: estimates.mean.point_estimate / NS_PER_MS,
        }
    }
}

/// Criterion benchmark estimates.
#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct Estimates {
    pub mean: Stat,
    pub median: Stat,
    #[serde(default)]
    pub std_dev: Option<Stat>,
    #[serde(default)]
    pub median_abs_dev: Option<Stat>,
    #[serde(default)]
    pub slope: Option<Stat>,
}

/// Statistical estimate with confidence interval.
#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct Stat {
    pub point_estimate: f64,
    #[serde(default)]
    pub standard_error: f64,
    pub confidence_interval: ConfidenceInterval,
}

/// Confidence interval bounds.
#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct ConfidenceInterval {
    #[serde(default)]
    pub confidence_level: f64,
    pub lower_bound: f64,
    pub upper_bound: f64,
}
