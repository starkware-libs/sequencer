use apollo_infra::metrics::RemoteServerMetrics;
use apollo_metrics::metrics::MetricQueryName;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    AlertSeverity,
    EvaluationRate,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

const MAX_CONNECTIONS: f64 = 80.0;

pub(crate) fn get_remote_server_number_of_connections_alert(
    component_name: &str,
    alert_group: EvaluationRate,
    metrics: &RemoteServerMetrics,
) -> Alert {
    Alert::new(
        format!("{component_name}_remote_server_number_of_connections"),
        format!("{component_name} - Remote server number of connections exceeds {MAX_CONNECTIONS}"),
        alert_group,
        format!(
            "sum by (namespace, pod) ({})",
            metrics.get_number_of_connections_metric().get_name_with_filter()
        ),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            MAX_CONNECTIONS,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::DayOnly,
        ObserverApplicability::Applicable,
    )
}
