use serde::Deserialize;

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
