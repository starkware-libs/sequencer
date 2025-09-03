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
use itertools::Itertools;
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

    pub(crate) fn from_counter(metric: &MetricCounter, panel_type: PanelType) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            vec![metric.get_name_with_filter().to_string()],
            panel_type,
        )
    }

    pub(crate) fn from_gauge(metric: &MetricGauge, panel_type: PanelType) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            vec![metric.get_name_with_filter().to_string()],
            panel_type,
        )
    }

    pub(crate) fn from_hist(metric: &MetricHistogram, panel_type: PanelType) -> Self {
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
            panel_type,
        )
    }

    // TODO(Tsabary): unify relevant parts with `from_hist` to avoid code duplication.
    pub(crate) fn from_request_type_labeled_hist(
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

pub(crate) fn create_request_type_labeled_hist_panels(
    metric: &LabeledMetricHistogram,
    panel_type: PanelType,
) -> HashMap<&str, Vec<Panel>> {
    metric
        .get_flat_label_values()
        .into_iter()
        .map(|request_label| {
            (
                request_label,
                Panel::from_request_type_labeled_hist(metric, panel_type, request_label),
            )
        })
        .into_group_map()
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

fn merge_panel_map<'a>(
    mut left: HashMap<&'a str, Vec<Panel>>,
    right: HashMap<&'a str, Vec<Panel>>,
) -> HashMap<&'a str, Vec<Panel>> {
    for (k, v) in right {
        left.entry(k).or_default().extend(v);
    }
    left
}

/// The returned value is a tuple of (regular panels, request type labeled panels grouped by
/// request type).
pub(crate) fn get_local_client_panels(
    local_client_metrics: &LocalClientMetrics,
) -> (Vec<Panel>, HashMap<&str, Vec<Panel>>) {
    (
        vec![],
        create_request_type_labeled_hist_panels(
            local_client_metrics.get_response_time_metric(),
            PanelType::TimeSeries,
        ),
    )
}

pub(crate) fn get_remote_client_panels(
    remote_client_metrics: &RemoteClientMetrics,
) -> (Vec<Panel>, HashMap<&str, Vec<Panel>>) {
    let attempts_panel =
        Panel::from_hist(remote_client_metrics.get_attempts_metric(), PanelType::TimeSeries);
    let response_times_panels = create_request_type_labeled_hist_panels(
        remote_client_metrics.get_response_time_metric(),
        PanelType::TimeSeries,
    );
    let communication_failure_times_panels = create_request_type_labeled_hist_panels(
        remote_client_metrics.get_communication_failure_time_metric(),
        PanelType::TimeSeries,
    );

    let mut labeled = response_times_panels;
    for (k, v) in communication_failure_times_panels {
        labeled.entry(k).or_default().extend(v);
    }

    (vec![attempts_panel], labeled)
}

pub(crate) fn get_local_server_panels(
    local_server_metrics: &LocalServerMetrics,
) -> (Vec<Panel>, HashMap<&str, Vec<Panel>>) {
    let received_msgs_panel =
        Panel::from_counter(local_server_metrics.get_received_metric(), PanelType::TimeSeries);
    let processed_msgs_panel =
        Panel::from_counter(local_server_metrics.get_processed_metric(), PanelType::TimeSeries);
    let queue_depth_panel =
        Panel::from_gauge(local_server_metrics.get_queue_depth_metric(), PanelType::TimeSeries);
    let processing_times_panels = create_request_type_labeled_hist_panels(
        local_server_metrics.get_processing_time_metric(),
        PanelType::TimeSeries,
    );
    let queueing_times_panels = create_request_type_labeled_hist_panels(
        local_server_metrics.get_queueing_time_metric(),
        PanelType::TimeSeries,
    );
    let mut labeled = processing_times_panels;
    for (k, v) in queueing_times_panels {
        labeled.entry(k).or_default().extend(v);
    }
    (vec![received_msgs_panel, processed_msgs_panel, queue_depth_panel], labeled)
}

pub(crate) fn get_remote_server_panels(
    remote_server_metrics: &RemoteServerMetrics,
) -> (Vec<Panel>, HashMap<&str, Vec<Panel>>) {
    let total_received_msgs_panel = Panel::from_counter(
        remote_server_metrics.get_total_received_metric(),
        PanelType::TimeSeries,
    );
    let valid_received_msgs_panel = Panel::from_counter(
        remote_server_metrics.get_valid_received_metric(),
        PanelType::TimeSeries,
    );
    let processed_msgs_panel =
        Panel::from_counter(remote_server_metrics.get_processed_metric(), PanelType::TimeSeries);
    let number_of_connections_panel = Panel::from_gauge(
        remote_server_metrics.get_number_of_connections_metric(),
        PanelType::TimeSeries,
    );
    (
        vec![
            total_received_msgs_panel,
            valid_received_msgs_panel,
            processed_msgs_panel,
            number_of_connections_panel,
        ],
        HashMap::new(),
    )
}

pub(crate) fn get_component_infra_row(row_name: &'static str, metrics: &InfraMetrics) -> Row {
    let (local_client_panels, local_client_request_type_labeled_panels) =
        get_local_client_panels(metrics.get_local_client_metrics());
    let (remote_client_panels, remote_client_request_type_labeled_panels) =
        get_remote_client_panels(metrics.get_remote_client_metrics());
    let (local_server_panels, local_server_request_type_labeled_panels) =
        get_local_server_panels(metrics.get_local_server_metrics());
    let (remote_server_panels, remote_server_request_type_labeled_panels) =
        get_remote_server_panels(metrics.get_remote_server_metrics());

    let client_panels = local_client_panels.into_iter().chain(remote_client_panels).collect_vec();
    let server_panels = local_server_panels.into_iter().chain(remote_server_panels).collect_vec();

    let grouped_client_panels = merge_panel_map(
        local_client_request_type_labeled_panels,
        remote_client_request_type_labeled_panels,
    );
    let grouped_server_panels = merge_panel_map(
        local_server_request_type_labeled_panels,
        remote_server_request_type_labeled_panels,
    );

    let mut panels: Vec<Panel> = Vec::new();
    panels.extend(client_panels);
    panels.extend(server_panels);
    for panels_vec in grouped_client_panels.into_values() {
        panels.extend(panels_vec);
    }
    for panels_vec in grouped_server_panels.into_values() {
        panels.extend(panels_vec);
    }

    // unstable sort is ok here because there are no duplicate panel names (unstable sort means
    // that the order of equal elements is not guaranteed)
    panels.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    Row::new(row_name, panels)
}
