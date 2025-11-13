use apollo_metrics::metric_definitions::METRIC_LABEL_FILTER;
use apollo_metrics::metrics::{MetricGauge, MetricScope};
use rstest::rstest;

use crate::query_builder::{increase, sum_by_label, DisplayMethod};

#[test]
fn increase_formats_correctly() {
    let m = MetricGauge::new(MetricScope::Batcher, "testing", "Fake description");
    let q = increase(&m, "5m");
    let expected = format!("increase(testing{METRIC_LABEL_FILTER}[5m])");
    assert_eq!(q, expected);
}

#[rstest]
#[case::raw_filtered(DisplayMethod::Raw, true)]
#[case::increase_filtered(DisplayMethod::Increase("5m"), true)]
#[case::raw_unfiltered(DisplayMethod::Raw, false)]
#[case::increase_unfiltered(DisplayMethod::Increase("15h"), false)]
fn sum_by_label_formats_correctly(#[case] display: DisplayMethod<'_>, #[case] filter_zeros: bool) {
    let m = MetricGauge::new(MetricScope::Batcher, "testing", "Fake description");
    let inner = match display {
        DisplayMethod::Increase(duration) => increase(&m, duration),
        DisplayMethod::Raw => format!("testing{METRIC_LABEL_FILTER}"),
    };
    let filter = match filter_zeros {
        true => " > 0",
        false => "",
    };
    let q = sum_by_label(&m, "label1", display, filter_zeros);
    let expected = format!("sum by (label1) ({inner}){filter}");
    assert_eq!(q, expected);
}
