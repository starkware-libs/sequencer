use crate::comparison::get_regression_percentage;
use crate::types::estimates::{ConfidenceInterval, Estimates, Stat};

#[test]
fn test_get_regression_percentage() {
    let estimates = Estimates {
        mean: Stat {
            point_estimate: 0.0706,
            standard_error: 0.01,
            confidence_interval: ConfidenceInterval {
                confidence_level: 0.95,
                lower_bound: 0.05,
                upper_bound: 0.09,
            },
        },
        median: Stat {
            point_estimate: 0.03,
            standard_error: 0.01,
            confidence_interval: ConfidenceInterval {
                confidence_level: 0.95,
                lower_bound: 0.01,
                upper_bound: 0.05,
            },
        },
        std_dev: None,
        median_abs_dev: None,
        slope: None,
    };

    let percentage = get_regression_percentage(&estimates);
    assert!((percentage - 7.06).abs() < 0.01);
}
