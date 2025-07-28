use apollo_metrics::metric_label_filter;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertEnvFiltering,
    AlertGroup,
    AlertLogicalOp,
    AlertSeverity,
};

const PENDING_DURATION_DEFAULT: &str = "30s";
const EVALUATION_INTERVAL_SEC_DEFAULT: u64 = 30;

pub(crate) fn get_general_pod_state_not_ready() -> Alert {
    Alert::new(
        "pod_state_not_ready",
        "Pod State Not Ready",
        AlertGroup::General,
        // Checks if a container in a pod is not ready (status_ready < 1).
        // Triggers when at least one container is unhealthy or not passing readiness probes.
        format!("kube_pod_container_status_ready{}", metric_label_filter!()),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::LessThan,
            comparison_value: 1.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

pub(crate) fn get_general_pod_state_crashloopbackoff() -> Alert {
    // Adding a 'reason' label to the metric label filter for 'CrashLoopBackOf' failures.
    // This is done by replacing the trailing '}' with ', reason="CrashLoopBackOff"}'.
    let metric_label_filter_with_reason = format!(
        "{}, reason=\"CrashLoopBackOff\"}}",
        metric_label_filter!().strip_suffix("}").expect("Metric label filter should end with a }")
    );
    Alert::new(
        "pod_state_crashloopbackoff",
        "Pod State CrashLoopBackOff",
        AlertGroup::General,
        format!(
            // Convert "NoData" to 0 using `absent`.
            "sum by(container, pod, namespace) (kube_pod_container_status_waiting_reason{}) or \
             absent(kube_pod_container_status_waiting_reason{}) * 0",
            metric_label_filter_with_reason, metric_label_filter_with_reason,
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 0.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_general_pod_memory_utilization(
    name: &str,
    title: &str,
    comparison_value: f64,
    severity: AlertSeverity,
) -> Alert {
    Alert::new(
        name,
        title,
        AlertGroup::General,
        format!(
            // Calculates the memory usage percentage of each container in a pod, relative to its
            // memory limit. This expression compares the actual memory usage
            // (working_set_bytes) of containers against their defined memory limits
            // (spec_memory_limit_bytes), and returns the result as a percentage.
            "max(container_memory_working_set_bytes{0}) by (container, pod, namespace) / \
             max(container_spec_memory_limit_bytes{0}) by (container, pod, namespace) * 100",
            metric_label_filter!()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        severity,
        AlertEnvFiltering::All,
    )
}

pub(crate) fn get_general_pod_memory_utilization_vec() -> Vec<Alert> {
    vec![
        get_general_pod_memory_utilization(
            "pod_state_high_memory_utilization",
            "Pod High Memory Utilization ( >70% )",
            70.0,
            AlertSeverity::DayOnly,
        ),
        get_general_pod_memory_utilization(
            "pod_state_critical_memory_utilization",
            "Pod Critical Memory Utilization ( >85% )",
            85.0,
            AlertSeverity::Regular,
        ),
    ]
}

pub(crate) fn get_general_pod_high_cpu_utilization() -> Alert {
    Alert::new(
        "pod_high_cpu_utilization",
        "Pod High CPU Utilization ( >90% )",
        AlertGroup::General,
        format!(
            // Calculates CPU usage rate over 2 minutes per container, compared to its defined CPU
            // quota. Showing CPU pressure.
            "max(irate(container_cpu_usage_seconds_total{0}[2m])) by (container, pod, namespace) \
             / (max(container_spec_cpu_quota{0}/100000) by (container, pod, namespace)) * 100",
            metric_label_filter!()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value: 90.0,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        AlertSeverity::Regular,
        AlertEnvFiltering::All,
    )
}

fn get_general_pod_disk_utilization(
    name: &str,
    title: &str,
    comparison_value: f64,
    severity: AlertSeverity,
) -> Alert {
    Alert::new(
        name,
        title,
        AlertGroup::General,
        format!(
            "max by (namespace,persistentvolumeclaim) (kubelet_volume_stats_used_bytes{0}) / (min \
             by (namespace,persistentvolumeclaim) (kubelet_volume_stats_available_bytes{0}) + max \
             by (namespace,persistentvolumeclaim) (kubelet_volume_stats_used_bytes{0}))*100",
            metric_label_filter!()
        ),
        vec![AlertCondition {
            comparison_op: AlertComparisonOp::GreaterThan,
            comparison_value,
            logical_op: AlertLogicalOp::And,
        }],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        severity,
        AlertEnvFiltering::All,
    )
}

pub(crate) fn get_general_pod_disk_utilization_vec() -> Vec<Alert> {
    vec![
        get_general_pod_disk_utilization(
            "pod_state_high_disk_utilization",
            "Pod High Disk Utilization ( >70% )",
            70.0,
            AlertSeverity::DayOnly,
        ),
        get_general_pod_disk_utilization(
            "pod_state_critical_disk_utilization",
            "Pod Critical Disk Utilization ( >90% )",
            90.0,
            AlertSeverity::Regular,
        ),
    ]
}
