use starknet_infra_utils::test_utils::assert_json_eq;

use crate::dashboard::{Alert, AlertComparisonOp, AlertCondition, AlertGroup, AlertLogicalOp};

#[test]
fn serialize_alert() {
    let alert = Alert {
        name: "Name",
        title: "Message",
        alert_group: AlertGroup::Batcher,
        expr: "max",
        conditions: &[AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: "5m",
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
        "for": "5m"
    });
    assert_json_eq(&serialized, &expected, "Json Comparison failed".to_string());
}
