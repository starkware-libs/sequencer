use apollo_infra_utils::test_utils::assert_json_eq;
use apollo_metrics::metrics::{MetricCounter, MetricScope};

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertEnvFiltering,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
    ObserverApplicability,
};
use crate::dashboard::{Panel, PanelType, ThresholdMode, ThresholdStep, Thresholds, Unit};

#[test]
fn serialize_alert() {
    let alert = Alert::new(
        "Name",
        "Message",
        AlertGroup::Batcher,
        "max".to_string(),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        "5m",
        20,
        AlertSeverity::Sos,
        ObserverApplicability::Applicable,
        AlertEnvFiltering::All,
    );

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
        "severity": "p1",
        "observer_applicable": true
    });
    assert_json_eq(&serialized, &expected, "Json Comparison failed".to_string());
}

#[test]
fn test_ratio_time_series() {
    let duration = "5m";
    let metric_1 = MetricCounter::new(MetricScope::Batcher, "r", "desc", 0);
    let metric_2 = MetricCounter::new(MetricScope::Batcher, "p", "desc", 0);
    let metric_3 = MetricCounter::new(MetricScope::Batcher, "a", "desc", 0);

    let panel =
        Panel::ratio_time_series("x", "x", &metric_1, &[&metric_1, &metric_2, &metric_3], duration)
            .with_log_query("Query");

    let expected = format!(
        "(increase({}[{duration}]) / (increase({}[{duration}]) + increase({}[{duration}]) + \
         increase({}[{duration}])))",
        metric_1.get_name_with_filter(),
        metric_1.get_name_with_filter(),
        metric_2.get_name_with_filter(),
        metric_3.get_name_with_filter(),
    );

    assert_eq!(panel.exprs, vec![expected]);
    assert_eq!(panel.extra.unit, Some(Unit::PercentUnit));
    assert_eq!(panel.extra.log_query, Some("Query".to_string()));

    let expected = format!(
        "(increase({}[{duration}]) / (increase({}[{duration}])))",
        metric_1.get_name_with_filter(),
        metric_2.get_name_with_filter(),
    );
    let panel = Panel::ratio_time_series("y", "y", &metric_1, &[&metric_2], duration);
    assert_eq!(panel.exprs, vec![expected]);
    assert!(!panel.extra.show_percent_change);
    assert!(panel.extra.log_query.is_none());
}

#[test]
fn test_extra_params() {
    let panel_with_extra_params = Panel::new("x", "x", vec!["y".to_string()], PanelType::Stat)
        .with_unit(Unit::Bytes)
        .show_percent_change()
        .with_log_query("Query")
        .with_absolute_thresholds(vec![
            ("green", None),
            ("red", Some(80.0)),
            ("yellow", Some(90.0)),
        ]);

    assert_eq!(panel_with_extra_params.extra.unit, Some(Unit::Bytes));
    assert!(panel_with_extra_params.extra.show_percent_change);
    assert_eq!(panel_with_extra_params.extra.log_query, Some("Query".to_string()));
    assert_eq!(
        panel_with_extra_params.extra.thresholds,
        Some(Thresholds {
            mode: ThresholdMode::Absolute,
            steps: vec![
                ThresholdStep { color: "green".to_string(), value: None },
                ThresholdStep { color: "red".to_string(), value: Some(80.0) },
                ThresholdStep { color: "yellow".to_string(), value: Some(90.0) },
            ],
        })
    );

    let panel_without_extra_params = Panel::new("x", "x", vec!["y".to_string()], PanelType::Stat);
    assert!(panel_without_extra_params.extra.unit.is_none());
    assert!(!panel_without_extra_params.extra.show_percent_change);
    assert!(panel_without_extra_params.extra.log_query.is_none());
    assert!(panel_without_extra_params.extra.thresholds.is_none());
}
