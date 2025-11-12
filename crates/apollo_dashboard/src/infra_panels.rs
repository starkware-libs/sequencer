use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_metrics::metrics::{LabeledMetricHistogram, MetricCommon};

use crate::dashboard::{Panel, PanelType, Row, HISTOGRAM_QUANTILES, HISTOGRAM_TIME_RANGE};

const INFRA_ROW_TITLE_SUFFIX: &str = "Infra";

pub(crate) fn get_component_infra_row(row_name: impl ToString, metrics: &InfraMetrics) -> Row {
    let mut panels: Vec<Panel> = Vec::new();
    // Add the general infra panels.
    panels.extend(UnlabeledPanels::from(metrics.get_local_client_metrics()).0);
    panels.extend(UnlabeledPanels::from(metrics.get_remote_client_metrics()).0);
    panels.extend(UnlabeledPanels::from(metrics.get_local_server_metrics()).0);
    panels.extend(UnlabeledPanels::from(metrics.get_remote_server_metrics()).0);

    let labeled_client_panels = get_infra_client_panels(
        metrics.get_local_client_metrics(),
        metrics.get_remote_client_metrics(),
    );
    let labeled_server_panels = get_infra_server_panels(
        metrics.get_local_server_metrics(),
        metrics.get_remote_server_metrics(),
    );
    assert!(
        labeled_client_panels.len() == labeled_server_panels.len(),
        "Number of labeled client and server panels must be equal, as there's a single panel per \
         request type."
    );
    // Add the client and server panels for each request type, next to each other.
    for (request_type_client_panel, request_type_server_panel) in
        labeled_client_panels.into_iter().zip(labeled_server_panels.into_iter())
    {
        panels.push(request_type_client_panel);
        panels.push(request_type_server_panel);
    }

    // Annotate the row name with infra row the suffix.
    let modified_row_name = format!("{} {INFRA_ROW_TITLE_SUFFIX}", row_name.to_string());
    Row::new(modified_row_name, panels)
}

// There is no equivalent for LabeledPanels because they are less straightforward than
// UnlabeledPanels and require an aggregation of metrics more often, for example the panels created
// using [`get_multi_metric_panel`].
/// A struct that contains all unlabeled panels for a given metrics struct.
struct UnlabeledPanels(Vec<Panel>);

impl From<&LocalClientMetrics> for UnlabeledPanels {
    fn from(_metrics: &LocalClientMetrics) -> Self {
        Self(vec![])
    }
}

impl From<&RemoteClientMetrics> for UnlabeledPanels {
    fn from(metrics: &RemoteClientMetrics) -> Self {
        Self(vec![Panel::from_hist(
            metrics.get_attempts_metric(),
            metrics.get_attempts_metric().get_name(),
            metrics.get_attempts_metric().get_description(),
        )])
    }
}

impl From<&LocalServerMetrics> for UnlabeledPanels {
    fn from(metrics: &LocalServerMetrics) -> Self {
        let received_msgs_panel =
            Panel::from_counter(metrics.get_received_metric(), PanelType::TimeSeries);
        let processed_msgs_panel =
            Panel::from_counter(metrics.get_processed_metric(), PanelType::TimeSeries);
        let queue_depth_panel = Panel::new(
            "local_queue_depth",
            "The depth of the local priority queues",
            vec![
                metrics.get_high_priority_queue_depth_metric().get_name_with_filter().to_string(),
                metrics.get_normal_priority_queue_depth_metric().get_name_with_filter().to_string(),
            ],
            PanelType::TimeSeries,
        );

        Self(vec![received_msgs_panel, processed_msgs_panel, queue_depth_panel])
    }
}

impl From<&RemoteServerMetrics> for UnlabeledPanels {
    fn from(metrics: &RemoteServerMetrics) -> Self {
        let total_received_msgs_panel =
            Panel::from_counter(metrics.get_total_received_metric(), PanelType::TimeSeries);
        let valid_received_msgs_panel =
            Panel::from_counter(metrics.get_valid_received_metric(), PanelType::TimeSeries);
        let processed_msgs_panel =
            Panel::from_counter(metrics.get_processed_metric(), PanelType::TimeSeries);
        let number_of_connections_panel =
            Panel::from_gauge(metrics.get_number_of_connections_metric(), PanelType::TimeSeries);

        Self(vec![
            total_received_msgs_panel,
            valid_received_msgs_panel,
            processed_msgs_panel,
            number_of_connections_panel,
        ])
    }
}

// For a given request label and vector of labeled histogram metrics, create a panel with multiple
// expressions.
fn get_multi_metric_panel(
    panel_name: &str,
    panel_description: &str,
    request_label: &str,
    metrics: &Vec<&LabeledMetricHistogram>,
    panel_type: PanelType,
) -> Panel {
    let mut exprs: Vec<String> = vec![];
    for metric in metrics {
        // TODO(alonl): func this (duplicate with from_request_type_labeled_hist)
        let metric_name_with_filter_and_reason = format!(
            "{}, {LABEL_NAME_REQUEST_VARIANT}=\"{request_label}\"}}",
            metric
                .get_name_with_filter()
                .strip_suffix("}")
                .expect("Metric label filter should end with a }")
        );
        exprs.extend(HISTOGRAM_QUANTILES.iter().map(|q| {
            format!(
                "histogram_quantile({q:.2},label_replace(sum by (le) \
                 (rate({metric_name_with_filter_and_reason}[{HISTOGRAM_TIME_RANGE}])), \
                 \"label_name\", \"{q:.2} {}\", \"le\", \".*\"))",
                metric.get_name()
            )
        }))
    }
    Panel::new(panel_name, panel_description, exprs, panel_type)
}

// This function assumes that all metrics share the same labels.
fn get_request_type_panels(
    labeled_metrics: &Vec<&LabeledMetricHistogram>,
    panel_class_name: &str,
) -> Vec<Panel> {
    let Some(first_metric) = labeled_metrics.first() else {
        return vec![];
    };
    let request_labels = first_metric.get_flat_label_values();

    let mut panels = vec![];
    for request_label in request_labels {
        let panel_name = format!("{} ({panel_class_name})", request_label);
        let panel_description =
            format!("{} infra metrics for request type {}", panel_class_name, request_label);
        let panel = get_multi_metric_panel(
            &panel_name,
            &panel_description,
            request_label,
            labeled_metrics,
            PanelType::TimeSeries,
        );
        panels.push(panel);
    }
    panels
}

fn get_infra_client_panels(
    local_client_metrics: &LocalClientMetrics,
    remote_client_metrics: &RemoteClientMetrics,
) -> Vec<Panel> {
    let mut labeled_metrics: Vec<&LabeledMetricHistogram> =
        local_client_metrics.get_all_labeled_metrics();
    labeled_metrics.extend(remote_client_metrics.get_all_labeled_metrics());
    get_request_type_panels(&labeled_metrics, "client")
}

fn get_infra_server_panels(
    local_server_metrics: &LocalServerMetrics,
    remote_server_metrics: &RemoteServerMetrics,
) -> Vec<Panel> {
    let mut labeled_metrics: Vec<&LabeledMetricHistogram> =
        local_server_metrics.get_all_labeled_metrics();
    labeled_metrics.extend(remote_server_metrics.get_all_labeled_metrics());
    get_request_type_panels(&labeled_metrics, "server")
}
