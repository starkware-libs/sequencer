use apollo_infra_utils::test_utils::assert_json_eq;
use apollo_metrics::metrics::{MetricCounter, MetricScope};

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
};
use crate::dashboard::Panel;

#[test]
fn serialize_alert() {
    let alert = Alert {
        name: "Name",
        title: "Message",
        alert_group: AlertGroup::Batcher,
        expr: "max".to_string(),
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: "5m",
        evaluation_interval_sec: 20,
        severity: AlertSeverity::Sos,
    };

    let serialized = serde_json::to_value(&alert).unwrap();
    let expected = serde_json::json!({
        "name": "Name",
        "title": "Message",
        "ruleGroup": "batcher",
        "expr": "max",
        "conditions": [
            {
                "evaluator": { "params": [10.0], "type": "gt" },
                "operator": { "type": "and" },
                "reducer": {"params": [], "type": "avg"},
                "type": "query"
            }
        ],
        "for": "5m",
        "intervalSec": 20,
        "severity": "p1"
    });
    assert_json_eq(&serialized, &expected, "Json Comparison failed".to_string());
}

#[test]
fn test_from_ratio() {
    let duration = "5m";
    let metric_1 = MetricCounter::new(MetricScope::Batcher, "r", "r_f", "desc", 0);
    let metric_2 = MetricCounter::new(MetricScope::Batcher, "p", "p_f", "desc", 0);
    let metric_3 = MetricCounter::new(MetricScope::Batcher, "a", "a_f", "desc", 0);

    let panel =
        Panel::_from_ratio("x", "x", &metric_1, &[&metric_1, &metric_2, &metric_3], duration);

    let expected = format!(
        "100 * (increase({}[{}]) / (increase({}[{}]) + increase({}[{}]) + increase({}[{}])))",
        metric_1.get_name_with_filter(),
        duration,
        metric_1.get_name_with_filter(),
        duration,
        metric_2.get_name_with_filter(),
        duration,
        metric_3.get_name_with_filter(),
        duration,
    );

    assert_eq!(panel.exprs, vec![expected]);

    let expected = format!(
        "100 * (increase({}[{}]) / (increase({}[{}])))",
        metric_1.get_name_with_filter(),
        duration,
        metric_2.get_name_with_filter(),
        duration,
    );
    let panel = Panel::_from_ratio("y", "y", &metric_1, &[&metric_2], duration);
    assert_eq!(panel.exprs, vec![expected]);
}
