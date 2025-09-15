use std::collections::HashMap;

use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_metrics::metrics::{
    LabeledMetricHistogram,
    MetricCounter,
    MetricGauge,
    MetricHistogram,
};
use indexmap::IndexMap;
use serde::ser::{SerializeMap, SerializeStruct};
use serde::{Serialize, Serializer};

#[cfg(test)]
#[path = "dashboard_test.rs"]
mod dashboard_test;

const HISTOGRAM_QUANTILES: &[f64] = &[0.50, 0.95];
const HISTOGRAM_TIME_RANGE: &str = "5m";

#[derive(Clone, Debug, PartialEq)]
pub struct Dashboard {
    name: &'static str,
    rows: Vec<Row>,
}

impl Dashboard {
    pub(crate) fn new(name: &'static str, rows: Vec<Row>) -> Self {
        Self { name, rows }
    }
}

// Custom Serialize implementation for Dashboard.
impl Serialize for Dashboard {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        let mut row_map = IndexMap::new();
        for row in &self.rows {
            row_map.insert(row.name, &row.panels);
        }

        map.serialize_entry(self.name, &row_map)?;
        map.end()
    }
}

/// Grafana panel types.
#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PanelType {
    Stat,
    TimeSeries,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Panel {
    name: String,
    description: String,
    exprs: Vec<String>,
    panel_type: PanelType,
}

impl Panel {
    pub(crate) fn new(
        name: impl ToString,
        description: impl ToString,
        exprs: Vec<String>,
        panel_type: PanelType,
    ) -> Self {
        // A panel assigns a unique id to each of its expressions. Conventionally, we use letters
        // A–Z, and for simplicity, we limit the number of expressions to this range.
        const NUM_LETTERS: u8 = b'Z' - b'A' + 1;
        let name = name.to_string();
        let description = description.to_string();
        assert!(
            exprs.len() <= NUM_LETTERS.into(),
            "Too many expressions ({} > {NUM_LETTERS}) in panel '{name}'.",
            exprs.len(),
        );
        Self { name, description, exprs, panel_type }
    }

    // TODO(Tsabary): unify relevant parts with `from_hist` to avoid code duplication.
    // TODO(alonl): remove the _ prefix once the function is used.
    pub(crate) fn _from_request_type_labeled_hist(
        metric: &LabeledMetricHistogram,
        panel_type: PanelType,
        request_label: &str,
    ) -> Self {
        let metric_name_with_filter_and_reason = format!(
            "{}, {LABEL_NAME_REQUEST_VARIANT}=\"{request_label}\"}}",
            metric
                .get_name_with_filter()
                .strip_suffix("}")
                .expect("Metric label filter should end with a }")
        );

        Self::new(
            format!("{} {request_label}", metric.get_name()),
            format!("{}: {request_label}", metric.get_description()),
            HISTOGRAM_QUANTILES
                .iter()
                .map(|q| {
                    format!(
                        "histogram_quantile({q:.2}, sum by (le) \
                         (rate({metric_name_with_filter_and_reason}[{HISTOGRAM_TIME_RANGE}])))",
                    )
                })
                .collect(),
            panel_type,
        )
    }

    pub(crate) fn ratio_time_series(
        name: &'static str,
        description: &'static str,
        numerator: &MetricCounter,
        denominator_parts: &[&MetricCounter],
        duration: &str,
    ) -> Self {
        let numerator_expr =
            format!("increase({}[{}])", numerator.get_name_with_filter(), duration);

        let denominator_expr = denominator_parts
            .iter()
            .map(|m| format!("increase({}[{}])", m.get_name_with_filter(), duration))
            .collect::<Vec<_>>()
            .join(" + ");

        let expr = format!("100 * ({} / ({}))", numerator_expr, denominator_expr);

        Self::new(name, description, vec![expr], PanelType::TimeSeries)
    }
}

impl From<&MetricCounter> for Panel {
    fn from(metric: &MetricCounter) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            vec![metric.get_name_with_filter().to_string()],
            PanelType::TimeSeries,
        )
    }
}

impl From<&MetricGauge> for Panel {
    fn from(metric: &MetricGauge) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            vec![metric.get_name_with_filter().to_string()],
            PanelType::TimeSeries,
        )
    }
}

impl From<&MetricHistogram> for Panel {
    fn from(metric: &MetricHistogram) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            HISTOGRAM_QUANTILES
                .iter()
                .map(|q| {
                    format!(
                        "histogram_quantile({q:.2}, sum by (le) \
                         (rate({}[{HISTOGRAM_TIME_RANGE}])))",
                        metric.get_name_with_filter(),
                    )
                })
                .collect(),
            PanelType::TimeSeries,
        )
    }
}

// There is no equivalent for LabeledPanels because they are less straightforward than
// UnlabeledPanels and require an aggregation of metrics more often, for example the panels created
// using `get_multi_metric_panel`.
/// A struct that contains all unlabeled panels for a given metrics struct.
struct UnlabeledPanels(Vec<Panel>);

impl From<&LocalClientMetrics> for UnlabeledPanels {
    fn from(_metrics: &LocalClientMetrics) -> Self {
        Self(vec![])
    }
}

impl From<&RemoteClientMetrics> for UnlabeledPanels {
    fn from(metrics: &RemoteClientMetrics) -> Self {
        Self(vec![Panel::from(metrics.get_attempts_metric())])
    }
}

impl From<&LocalServerMetrics> for UnlabeledPanels {
    fn from(metrics: &LocalServerMetrics) -> Self {
        let received_msgs_panel = Panel::from(metrics.get_received_metric());
        let processed_msgs_panel = Panel::from(metrics.get_processed_metric());
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
        let total_received_msgs_panel = Panel::from(metrics.get_total_received_metric());
        let valid_received_msgs_panel = Panel::from(metrics.get_valid_received_metric());
        let processed_msgs_panel = Panel::from(metrics.get_processed_metric());
        let number_of_connections_panel = Panel::from(metrics.get_number_of_connections_metric());

        Self(vec![
            total_received_msgs_panel,
            valid_received_msgs_panel,
            processed_msgs_panel,
            number_of_connections_panel,
        ])
    }
}

pub(crate) fn _create_request_type_labeled_hist_panels(
    metric: &LabeledMetricHistogram,
    panel_type: PanelType,
) -> Vec<Panel> {
    metric
        .get_flat_label_values()
        .into_iter()
        .map(|request_label| {
            Panel::_from_request_type_labeled_hist(metric, panel_type, request_label)
        })
        .collect()
}

// For a given request label and vector of labeled histogram metrics, create a panel with multiple
// expressions.
pub(crate) fn get_multi_metric_panel(
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

// Custom Serialize implementation for Panel.
impl Serialize for Panel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Panel", 5)?; // 5 fields (including extra dict)
        state.serialize_field("title", &self.name)?;
        state.serialize_field("description", &self.description)?;
        state.serialize_field("type", &self.panel_type)?;
        state.serialize_field("exprs", &self.exprs)?;

        // Append an empty dictionary `{}` at the end
        let empty_map: HashMap<String, String> = HashMap::new();
        state.serialize_field("extra_params", &empty_map)?;

        state.end()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    name: &'static str,
    panels: Vec<Panel>,
}

impl Row {
    pub(crate) const fn new(name: &'static str, panels: Vec<Panel>) -> Self {
        Self { name, panels }
    }
}

// Custom Serialize implementation for Row.
impl Serialize for Row {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(self.name, &self.panels)?;
        map.end()
    }
}

// This function assumes that all metrics share the same labels.
fn get_request_type_labeled_panels(
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

pub(crate) fn get_labeled_client_panels(
    local_client_metrics: &LocalClientMetrics,
    remote_client_metrics: &RemoteClientMetrics,
) -> Vec<Panel> {
    let mut labeled_metrics: Vec<&LabeledMetricHistogram> =
        local_client_metrics.get_all_labeled_metrics();
    labeled_metrics.extend(remote_client_metrics.get_all_labeled_metrics());
    get_request_type_labeled_panels(&labeled_metrics, "client")
}

pub(crate) fn get_labeled_server_panels(
    local_server_metrics: &LocalServerMetrics,
    remote_server_metrics: &RemoteServerMetrics,
) -> Vec<Panel> {
    let mut labeled_metrics: Vec<&LabeledMetricHistogram> =
        local_server_metrics.get_all_labeled_metrics();
    labeled_metrics.extend(remote_server_metrics.get_all_labeled_metrics());
    get_request_type_labeled_panels(&labeled_metrics, "server")
}

pub(crate) fn get_component_infra_row(row_name: &'static str, metrics: &InfraMetrics) -> Row {
    let labeled_client_panels = get_labeled_client_panels(
        metrics.get_local_client_metrics(),
        metrics.get_remote_client_metrics(),
    );
    let labeled_server_panels = get_labeled_server_panels(
        metrics.get_local_server_metrics(),
        metrics.get_remote_server_metrics(),
    );

    let mut panels: Vec<Panel> = Vec::new();
    panels.extend(UnlabeledPanels::from(metrics.get_local_client_metrics()).0);
    panels.extend(UnlabeledPanels::from(metrics.get_remote_client_metrics()).0);
    panels.extend(UnlabeledPanels::from(metrics.get_local_server_metrics()).0);
    panels.extend(UnlabeledPanels::from(metrics.get_remote_server_metrics()).0);
    panels.extend(labeled_client_panels);
    panels.extend(labeled_server_panels);

    Row::new(row_name, panels)
}
