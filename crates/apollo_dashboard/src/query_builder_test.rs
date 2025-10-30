use apollo_infra_utils::template::Template;
use apollo_metrics::metric_label_filter;
use apollo_metrics::metrics::{MetricGauge, MetricScope};

use crate::query_builder;

#[test]
fn increase_formats_correctly() {
    let m = MetricGauge::new(MetricScope::Batcher, "testing", "Fake description");
    let q = query_builder::increase(&m, "5m");
    let expected =
        Template::new("increase({}{}[{}])").format(&[&"testing", &metric_label_filter!(), &"5m"]);
    assert_eq!(q, expected);
}
