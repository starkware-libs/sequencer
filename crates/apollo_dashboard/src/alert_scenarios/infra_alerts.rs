use apollo_metrics::define_metrics;
use apollo_metrics::metrics::MetricQueryName;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    AlertSeverity,
    EvaluationRate,
    ObserverApplicability,
    PENDING_DURATION_DEFAULT,
};

define_metrics!(
    Pod => {
        MetricCounter { CONTAINER_CPU_USAGE_SECONDS_TOTAL, "container_cpu_usage_seconds_total", "Cumulative CPU time consumed by the container in seconds. Use with irate() for CPU usage rate.", init=0 },
        MetricGauge { CONTAINER_MEMORY_WORKING_SET_BYTES, "container_memory_working_set_bytes", "Estimated amount of memory that cannot be evicted (working set: recently accessed memory, dirty memory, kernel memory). Used by the OOM killer for eviction decisions." },
        MetricGauge { CONTAINER_SPEC_CPU_QUOTA, "container_spec_cpu_quota", "CPU quota in microseconds allocated to the container from the container spec (cgroups)." },
        MetricGauge { CONTAINER_SPEC_MEMORY_LIMIT_BYTES, "container_spec_memory_limit_bytes", "Memory limit for the container from the container spec, in bytes. Exceeding this limit can trigger OOM kill." },
        MetricGauge { KUBE_POD_CONTAINER_STATUS_READY, "kube_pod_container_status_ready", "Indicates whether a specific container within a pod has successfully passed its readiness check (Readiness Probe) and is currently ready to serve network traffic." },
        MetricGauge { KUBE_POD_CONTAINER_STATUS_WAITING_REASON, "kube_pod_container_status_waiting_reason", "Indicates the reason a container is in a waiting state (e.g., ContainerCreating, ImagePullBackOff, CrashLoopBackOff). This means the container process has not started or has crashed." },
        MetricGauge { KUBELET_VOLUME_STATS_AVAILABLE_BYTES, "kubelet_volume_stats_available_bytes", "Number of bytes available on the volume (persistent volume claim)." },
        MetricGauge { KUBELET_VOLUME_STATS_USED_BYTES, "kubelet_volume_stats_used_bytes", "Number of bytes used by the volume (persistent volume claim)." },
    },
);

pub(crate) fn get_general_pod_state_not_ready() -> Alert {
    Alert::new(
        "pod_state_not_ready",
        "Pod status Not Ready",
        EvaluationRate::High,
        // kube_pod_container_status_ready value is 0 if the container is 'not ready', and 1
        // if the container is 'ready'. We replace the pod name with 'service_name' to group by the
        // service name using the label_replace function and the regex pattern. Summing across all
        // service names results in 0 if all of its relevant pods are 'not ready'.
        format!(
            "sum by (service_name) (label_replace({}, \"service_name\", \"$1\", \"pod\", \
             \"^sequencer-(.+)-(?:deployment|statefulset)-[a-z0-9]+(?:-[a-z0-9]+)?$\"))",
            KUBE_POD_CONTAINER_STATUS_READY.get_name_with_filter()
        ),
        // There is no "equal to" operator in Grafana, and the reported values are integers.
        // Therefore, we use a less-than comparison with with a lower-than-1 threshold. The
        // intention here is to check for equality with 0.
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 0.1, AlertLogicalOp::And)],
        // Spot evictions in GKE have a 30 second notification window, in which the pod state is
        // marked as not ready. To avoid alerting on these, we require the not-ready status to last
        // longer (e.g., 60 seconds).
        "60s",
        AlertSeverity::Regular,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_general_pod_state_crashloopbackoff() -> Alert {
    Alert::new(
        "pod_state_crashloopbackoff",
        "Pod status CrashLoopBackOff",
        EvaluationRate::Default,
        format!(
            // Convert "NoData" to 0 using `absent`.
            "sum by(container, pod, namespace) ({}) or absent({}) * 0",
            KUBE_POD_CONTAINER_STATUS_WAITING_REASON
                .get_name_with_filer_and_additional_fields("reason=\"CrashLoopBackOff\""),
            KUBE_POD_CONTAINER_STATUS_WAITING_REASON
                .get_name_with_filer_and_additional_fields("reason=\"CrashLoopBackOff\""),
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        AlertSeverity::Regular,
        ObserverApplicability::Applicable,
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
        EvaluationRate::Default,
        format!(
            // Calculates the memory usage percentage of each container in a pod, relative to its
            // memory limit. This expression compares the actual memory usage
            // (working_set_bytes) of containers against their defined memory limits
            // (spec_memory_limit_bytes), and returns the result as a percentage.
            "max({}) by (container, pod, namespace) / max({}) by (container, pod, namespace) * 100",
            CONTAINER_MEMORY_WORKING_SET_BYTES.get_name_with_filter(),
            CONTAINER_SPEC_MEMORY_LIMIT_BYTES.get_name_with_filter(),
        ),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            comparison_value,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        severity,
        ObserverApplicability::Applicable,
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
        EvaluationRate::Default,
        format!(
            // Calculates CPU usage rate over 2 minutes per container, compared to its defined CPU
            // quota. Showing CPU pressure.
            "max(irate({}[2m])) by (container, pod, namespace) / (max({}/100000) by (container, \
             pod, namespace)) * 100",
            CONTAINER_CPU_USAGE_SECONDS_TOTAL.get_name_with_filter(),
            CONTAINER_SPEC_CPU_QUOTA.get_name_with_filter(),
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 90.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        AlertSeverity::Regular,
        ObserverApplicability::Applicable,
    )
}

// The prediction horizon for the disk-filling-soon alert: 2 weeks expressed in seconds.
// Derived as: 14 days * 24 hours/day * 3600 seconds/hour = 1_209_600 seconds.
const DISK_FILLING_HORIZON_SECONDS: u64 = 14 * 24 * 3600;

fn get_pod_disk_utilization_alert() -> Alert {
    Alert::new(
        "pod_state_critical_disk_utilization",
        "Pod Critical Disk Utilization ( >90% )",
        EvaluationRate::Low,
        format!(
            "max by (namespace,persistentvolumeclaim) ({}) / (min by \
             (namespace,persistentvolumeclaim) ({}) + max by (namespace,persistentvolumeclaim) \
             ({}))*100",
            KUBELET_VOLUME_STATS_USED_BYTES.get_name_with_filter(),
            KUBELET_VOLUME_STATS_AVAILABLE_BYTES.get_name_with_filter(),
            KUBELET_VOLUME_STATS_USED_BYTES.get_name_with_filter(),
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 90.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::Applicable,
    )
}

fn get_pod_disk_filling_soon_alert() -> Alert {
    Alert::new(
        "pod_state_disk_filling_soon",
        "Pod Disk Filling Soon (predicted full within 2 weeks)",
        AlertGroup::General,
        format!(
            // Projects disk usage {horizon} seconds ahead using linear regression over the last
            // 2 days of used_bytes samples. The result is divided by the total provisioned
            // capacity (available_bytes + used_bytes). Unlike used_bytes, this sum is constant
            // because it equals the fixed provisioned volume size — it does not change as the
            // disk fills up. The alert fires when the ratio exceeds 1, meaning the disk is
            // projected to be full.
            "max by (namespace,persistentvolumeclaim) (predict_linear({}[2d], {horizon})) / (min \
             by (namespace,persistentvolumeclaim) ({}) + max by (namespace,persistentvolumeclaim) \
             ({}))",
            KUBELET_VOLUME_STATS_USED_BYTES.get_name_with_filter(),
            KUBELET_VOLUME_STATS_AVAILABLE_BYTES.get_name_with_filter(),
            KUBELET_VOLUME_STATS_USED_BYTES.get_name_with_filter(),
            horizon = DISK_FILLING_HORIZON_SECONDS,
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        AlertSeverity::WorkingHours,
        ObserverApplicability::Applicable,
    )
}

pub(crate) fn get_general_pod_disk_utilization_vec() -> Vec<Alert> {
    vec![get_pod_disk_utilization_alert(), get_pod_disk_filling_soon_alert()]
}

pub(crate) fn get_periodic_ping() -> Alert {
    Alert::new(
        "periodic_ping",
        "Periodic Ping",
        EvaluationRate::Default,
        // Checks if the UTC time is 7:55 AM on Sunday.
        "(day_of_week() == bool 0) * (hour() == bool 7) * (minute() == bool 55)",
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        "0s",
        AlertSeverity::Regular,
        ObserverApplicability::Applicable,
    )
}
