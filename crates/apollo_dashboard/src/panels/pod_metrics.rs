use apollo_metrics::metric_definitions::METRIC_LABEL_FILTER;

use crate::dashboard::Row;
use crate::infra_panels::POD_LEGEND;
use crate::panel::{Panel, PanelType, Unit};

// TODO(Tsabary): replace query building with relevant functions and templates.

pub(crate) fn get_pod_metrics_row() -> Row {
    Row::new(
        "Pod Metrics",
        vec![
            get_pod_cpu_request_utilization_panel(),
            get_pod_cpu_throttling_panel(),
            get_pod_memory_request_utilization_panel(),
            get_pod_memory_limit_utilization_panel(),
            get_pod_disk_utilization_panel(),
            get_pod_disk_limit_utilization_panel(),
        ],
    )
}

const POD_METRICS_DEFAULT_DURATION: &str = "5m";

// ---------------------------- CPU ----------------------------

// Pod CPU utilization as a ratio of:
//   total CPU usage rate of containers in the pod (in cores)
//   --------------------------------------------------------
//   total CPU cores requested by containers in the pod
// Aggregated per (namespace, pod), the result is a value between 0.0 and 1.0 per pod.
// Interpreted as: "How much of its requested CPU is this pod actually using?"
fn get_pod_cpu_request_utilization_panel() -> Panel {
    Panel::new(
        "Pod CPU Request Utilization",
        format!("Pod CPU utilization (usage / requests) ({POD_METRICS_DEFAULT_DURATION} window)"),
        format!(
            "
            (
                sum by (namespace, pod) (
                    rate(container_cpu_usage_seconds_total{METRIC_LABEL_FILTER}[{POD_METRICS_DEFAULT_DURATION}])
                )
            )
            /
            (
                sum by (namespace, pod) (
                    kube_pod_container_resource_requests_cpu_cores{METRIC_LABEL_FILTER}
                )
            )
            "
        ),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::PercentUnit)
    .with_absolute_thresholds(pod_metric_thresholds())
}

// Pod CPU throttling as a ratio of:
//   number of CFS CPU periods where containers in the pod were throttled
//   --------------------------------------------------------------------
//   total number of CFS CPU periods for containers in the pod
// Aggregated per (namespace, pod), the result is a value between 0.0 and 1.0 per pod.
// Interpreted as: "What fraction of time is this pod being CPU-throttled by its CPU *limit*?"
fn get_pod_cpu_throttling_panel() -> Panel {
    Panel::new(
        "Pod CPU throttling",
        format!("Pod CPU throttling (throttled / total periods) ({POD_METRICS_DEFAULT_DURATION} window)"),
        format!(
            "(
                sum by (namespace, pod) (
                    rate(container_cpu_cfs_throttled_periods_total{METRIC_LABEL_FILTER}[{POD_METRICS_DEFAULT_DURATION}])
                )
            )
            /
            (
                sum by (namespace, pod) (
                    rate(container_cpu_cfs_periods_total{METRIC_LABEL_FILTER}[{POD_METRICS_DEFAULT_DURATION}])
                )
            )
            "
        ),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::PercentUnit)
    .with_absolute_thresholds(pod_metric_thresholds())
}

// ---------------------------- MEMORY ----------------------------

// Pod memory utilization as a ratio of:
//   total memory used by containers in the pod
//   ------------------------------------------------
//   total memory requested by containers in the pod
// Aggregated per (namespace, pod), the result is a value between 0.0 and 1.0 per pod.
// Interpreted as: "How much of its requested memory is this pod actually using?"
fn get_pod_memory_request_utilization_panel() -> Panel {
    Panel::new(
        "Pod Memory Request Utilization",
        "Pod memory utilization (used / requests)",
        format!(
            "
            (
                sum by (namespace, pod) (
                    container_memory_working_set_bytes{METRIC_LABEL_FILTER}
                )
            )
            /
            (
                sum by (namespace, pod) (
                    kube_pod_container_resource_requests_memory_bytes{METRIC_LABEL_FILTER}
                )
            )
            "
        ),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::PercentUnit)
    .with_absolute_thresholds(pod_metric_thresholds())
}

// Pod memory limit utilization as a ratio of:
//   total memory used by containers in the pod
//   ------------------------------------------
//   total memory limit of containers in the pod
// Aggregated per (namespace, pod), the result is a value between 0.0 and 1.0 per pod.
// Interpreted as: "How close is this pod to its memory *limit* (OOM-kill threshold)?"
// Note: memory is not throttled like CPU; crossing this limit results in OOM kills.
fn get_pod_memory_limit_utilization_panel() -> Panel {
    Panel::new(
        "Pod Memory Limit Utilization",
        "Pod memory limit utilization (used / limits)",
        format!(
            "
            (
                sum by (namespace, pod) (
                    container_memory_working_set_bytes{METRIC_LABEL_FILTER}
                )
            )
            /
            (
                sum by (namespace, pod) (
                    container_spec_memory_limit_bytes{METRIC_LABEL_FILTER}
                )
            )
            "
        ),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::PercentUnit)
    .with_absolute_thresholds(pod_metric_thresholds())
}

// ---------------------------- DISK ----------------------------

// Pod disk utilization (PVC) as a ratio of:
//   total volume bytes used by the pod
//   ----------------------------------
//   total volume capacity bytes of the pod
// Aggregated per (namespace, pod), the result is a value between 0.0 and 1.0 per pod.
// Interpreted as: "How much of the provisioned PVC capacity is this pod using?"
fn get_pod_disk_utilization_panel() -> Panel {
    Panel::new(
        "Pod Disk Utilization",
        "Pod disk utilization (used / capacity)",
        format!(
            "
            (
                sum by (namespace, pod) (
                    kubelet_volume_stats_used_bytes{METRIC_LABEL_FILTER}
                )
            )
            /
            (
                sum by (namespace, pod) (
                    kubelet_volume_stats_capacity_bytes{METRIC_LABEL_FILTER}
                )
            )
            "
        ),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::PercentUnit)
    .with_absolute_thresholds(pod_metric_thresholds())
}

// Pod disk limit utilization (PVC) as a ratio of:
//   total volume bytes used by the pod
//   ----------------------------------
//   total volume capacity bytes of the pod (effective disk limit)
// Aggregated per (namespace, pod), the result is a value between 0.0 and 1.0 per pod.
// Interpreted as: "How close is this pod's PVC storage to being full (disk *limit* saturation)?"
fn get_pod_disk_limit_utilization_panel() -> Panel {
    Panel::new(
        "Pod Disk Limit Utilization",
        "Pod disk limit utilization (used / capacity)",
        format!(
            "
            (
                sum by (namespace, pod) (
                    kubelet_volume_stats_used_bytes{METRIC_LABEL_FILTER}
                )
            )
            /
            (
                sum by (namespace, pod) (
                    kubelet_volume_stats_capacity_bytes{METRIC_LABEL_FILTER}
                )
            )
            "
        ),
        PanelType::TimeSeries,
    )
    .with_legends(POD_LEGEND)
    .with_unit(Unit::PercentUnit)
    .with_absolute_thresholds(pod_metric_thresholds())
}

fn pod_metric_thresholds() -> Vec<(&'static str, Option<f64>)> {
    vec![("green", None), ("yellow", Some(0.6)), ("red", Some(0.8))]
}
