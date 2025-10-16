use serde::Deserialize;

/// Criterion benchmark estimates.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Estimates {
    pub mean: Stat,
    pub median: Stat,
    pub std_dev: Stat,
    pub median_abs_dev: Stat,
    pub slope: Option<Stat>,
}

/// Statistical estimate with confidence interval.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Stat {
    pub point_estimate: f64,
    pub standard_error: f64,
    pub confidence_interval: ConfidenceInterval,
}

/// Confidence interval bounds.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ConfidenceInterval {
    pub confidence_level: f64,
    pub lower_bound: f64,
    pub upper_bound: f64,
}
