use starknet_infra_utils::test_utils::assert_json_eq;

use crate::dashboard::{Alert, AlertComparisonOp, AlertCondition, AlertLogicalOp};

#[test]
fn serialize_alert() {
    let alert = Alert {
        name: "Name",
        message: "Message",
        conditions: &[AlertCondition {
            expr: "max",
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 10.0,
            logical_op: AlertLogicalOp::And,
        }],
        pending_duration: "5m",
    };

    let serialized = serde_json::to_value(&alert).unwrap();
    let expected = serde_json::json!({
        "name": "Name",
        "message": "Message",
        "conditions": [
            {
                "evaluator": { "params": [10.0], "type": "gt" },
                "operator": { "type": "and" },
                "query": { "expr": "max" }
            }
        ],
        "for": "5m"
    });
    assert_json_eq(&serialized, &expected, "Json Comparison failed".to_string());
}
