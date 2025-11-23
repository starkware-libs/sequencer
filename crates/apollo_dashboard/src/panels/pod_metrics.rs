use apollo_metrics::metric_label_filter;

use crate::dashboard::{Panel, PanelType, Row};

fn get_pod_memory_utilization_panel() -> Panel {
    Panel::new(
        "pod_memory_utilization",
        "Pod Memory Utilization",
        vec![format!("container_memory_working_set_bytes{0}", metric_label_filter!())],
        PanelType::TimeSeries,
    )
}

fn get_pod_disk_utilization_panel() -> Panel {
    Panel::new(
        "pod_disk_utilization",
        "Pod Disk Utilization",
        vec![format!("kubelet_volume_stats_used_bytes{0}", metric_label_filter!())],
        PanelType::TimeSeries,
    )
}

fn get_pod_cpu_utilization_panel() -> Panel {
    Panel::new(
        "pod_cpu_utilization",
        "Pod CPU Utilization",
        vec![format!("container_cpu_usage_seconds_total{0}", metric_label_filter!())],
        PanelType::TimeSeries,
    )
}

pub(crate) fn get_pod_metrics_row() -> Row {
    Row::new(
        "Pod Metrics",
        vec![
            get_pod_memory_utilization_panel(),
            get_pod_disk_utilization_panel(),
            get_pod_cpu_utilization_panel(),
        ],
    )
}
