use apollo_gateway::metrics::GATEWAY_TRANSACTIONS_RECEIVED;
use const_format::formatcp;

use crate::dashboard::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertGroup,
    AlertLogicalOp,
    Alerts,
};

pub const DEV_ALERTS_JSON_PATH: &str = "Monitoring/sequencer/dev_grafana_alerts.json";

const GATEWAY_ADD_TX_RATE_DROP: Alert = Alert {
    name: "gateway_add_tx_rate_drop",
    title: "Gateway add_tx rate drop",
    alert_group: AlertGroup::Gateway,
    expr: formatcp!("sum(rate({}[20m]))", GATEWAY_TRANSACTIONS_RECEIVED.get_name()),
    conditions: &[AlertCondition {
        comparison_op: AlertComparisonOp::LessThan,
        comparison_value: 0.01,
        logical_op: AlertLogicalOp::And,
    }],
    pending_duration: "5m",
};

pub const SEQUENCER_ALERTS: Alerts = Alerts::new(&[GATEWAY_ADD_TX_RATE_DROP]);
