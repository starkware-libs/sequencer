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

pub const HISTOGRAM_QUANTILES: &[f64] = &[0.50, 0.95];
pub const HISTOGRAM_TIME_RANGE: &str = "5m";

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
    #[allow(dead_code)] // TODO(Ron): use BarGauge in panels
    BarGauge,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Unit {
    #[allow(dead_code)] // TODO(Ron): use Bytes in panels
    Bytes,
    #[allow(dead_code)] // TODO(Ron): use Seconds in panels
    Seconds,
    #[allow(dead_code)] // TODO(Ron): use Percent in panels
    #[allow(clippy::enum_variant_names)]
    // The expected values for PercentUnit are [0,1]
    PercentUnit,
}

impl Unit {
    fn grafana_id(&self) -> &'static str {
        match self {
            Unit::Bytes => "bytes",
            Unit::Seconds => "s",
            Unit::PercentUnit => "percentunit",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThresholdMode {
    #[allow(dead_code)] // TODO(Ron): use in panels
    Absolute,
    #[allow(dead_code)] // TODO(Ron): use in panels
    Percentage,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ThresholdStep {
    pub color: String,
    pub value: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Thresholds {
    pub mode: ThresholdMode,
    pub steps: Vec<ThresholdStep>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ExtraParams {
    pub unit: Option<Unit>,
    pub show_percent_change: bool,
    pub log_query: Option<String>,
    pub thresholds: Option<Thresholds>,
    pub legends: Option<Vec<String>>,
}

impl ExtraParams {
    fn to_string_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        if let Some(u) = &self.unit {
            map.insert("unit".into(), u.grafana_id().into());
        }
        if self.show_percent_change {
            map.insert("showPercentChange".into(), "true".into());
        }
        if let Some(lq) = &self.log_query {
            map.insert("log_query".to_string(), lq.clone());
        }
        if let Some(th) = &self.thresholds {
            let json = serde_json::to_string(th).unwrap();
            map.insert("thresholds".into(), json);
        }
        if let Some(legends) = &self.legends {
            let json = serde_json::to_string(legends).unwrap();
            map.insert("legends".into(), json);
        }
        map
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Panel {
    name: String,
    description: String,
    exprs: Vec<String>,
    panel_type: PanelType,
    extra: ExtraParams,
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
        Self { name, description, exprs, panel_type, extra: ExtraParams::default() }
    }
    #[allow(dead_code)] // TODO(Ron): use in panels
    pub fn with_unit(mut self, unit: Unit) -> Self {
        self.extra.unit = Some(unit);
        self
    }
    #[allow(dead_code)] // TODO(Ron): use in panels
    pub fn show_percent_change(mut self) -> Self {
        assert_eq!(
            self.panel_type,
            PanelType::Stat,
            "showPercentChange is only supported on Stat panels; got {:?}",
            self.panel_type
        );
        self.extra.show_percent_change = true;
        self
    }
    #[allow(dead_code)] // TODO(Ron): use in panels
    pub fn with_log_query(mut self, log_query: impl Into<String>) -> Self {
        self.extra.log_query = Some(log_query.into());
        self
    }

    #[allow(dead_code)] // TODO(Ron): use in panels
    pub fn with_legends<S: Into<String>>(mut self, legends: Vec<S>) -> Self {
        assert_eq!(
            legends.len(),
            self.exprs.len(),
            "Number of legends must match number of expressions"
        );
        self.extra.legends = Some(legends.into_iter().map(|s| s.into()).collect());
        self
    }

    #[allow(dead_code)] // TODO(Ron): use in panels
    fn with_thresholds(mut self, mode: ThresholdMode, steps: Vec<(&str, Option<f64>)>) -> Self {
        assert!(!steps.is_empty(), "thresholds must include at least one step");
        assert!(steps[0].1.is_none(), "first threshold step must have value=null");
        for w in steps.windows(2).skip(1) {
            let prev = w[0].1.unwrap();
            let next = w[1].1.unwrap();
            assert!(
                next > prev,
                "threshold values must be strictly increasing: {} then {}",
                prev,
                next
            );
        }
        let steps = steps
            .into_iter()
            .map(|(color, value)| ThresholdStep { color: color.to_string(), value })
            .collect();
        self.extra.thresholds = Some(Thresholds { mode, steps });
        self
    }

    #[allow(dead_code)] // TODO(Ron): use in panels
    /// - The first step must have `value = None` → becomes `null` in Grafana. This defines the base
    ///   color for all values below the first numeric threshold.
    /// - All following steps must be `Some(number)` with **strictly increasing values**. Grafana
    ///   chooses the color of the last threshold whose value ≤ datapoint.
    /// - Colors may be any valid CSS color string:
    /// - Named: "green", "red": <https://developer.mozilla.org/en-US/docs/Web/CSS/named-color>.
    /// - Hex: "#FF0000", "#00ff00".
    /// - RGB/HSL: "rgb(255,0,0)", "hsl(120,100%,50%)", etc.
    pub fn with_absolute_thresholds(self, steps: Vec<(&str, Option<f64>)>) -> Self {
        self.with_thresholds(ThresholdMode::Absolute, steps)
    }

    #[allow(dead_code)] // TODO(Ron): use in panels
    pub fn with_percentage_thresholds(self, steps: Vec<(&str, Option<f64>)>) -> Self {
        self.with_thresholds(ThresholdMode::Percentage, steps)
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

        let expr = format!("({} / ({}))", numerator_expr, denominator_expr);

        Self::new(name, description, vec![expr], PanelType::TimeSeries).with_unit(Unit::PercentUnit)
    }
}

#[allow(dead_code)] // TODO(Ron): use in panels
pub fn traffic_light_thresholds(yellow: f64, red: f64) -> Vec<(&'static str, Option<f64>)> {
    vec![("green", None), ("yellow", Some(yellow)), ("red", Some(red))]
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
        state.serialize_field("extra_params", &self.extra.to_string_map())?;

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

pub(crate) fn get_unlabeled_local_client_panels(
    _local_client_metrics: &LocalClientMetrics,
) -> Vec<Panel> {
    vec![]
}

pub(crate) fn get_unlabeled_remote_client_panels(
    remote_client_metrics: &RemoteClientMetrics,
) -> Vec<Panel> {
    vec![Panel::from_hist(remote_client_metrics.get_attempts_metric(), PanelType::TimeSeries)]
}

pub(crate) fn get_unlabeled_local_server_panels(
    local_server_metrics: &LocalServerMetrics,
) -> Vec<Panel> {
    let received_msgs_panel =
        Panel::from_counter(local_server_metrics.get_received_metric(), PanelType::TimeSeries);
    let processed_msgs_panel =
        Panel::from_counter(local_server_metrics.get_processed_metric(), PanelType::TimeSeries);
    let queue_depth_panel = Panel::new(
        "local_queue_depth",
        "The depth of the local priority queues",
        vec![
            local_server_metrics
                .get_high_priority_queue_depth_metric()
                .get_name_with_filter()
                .to_string(),
            local_server_metrics
                .get_normal_priority_queue_depth_metric()
                .get_name_with_filter()
                .to_string(),
        ],
        PanelType::TimeSeries,
    );

    vec![received_msgs_panel, processed_msgs_panel, queue_depth_panel]
}

pub(crate) fn get_unlabeled_remote_server_panels(
    remote_server_metrics: &RemoteServerMetrics,
) -> Vec<Panel> {
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

    vec![
        total_received_msgs_panel,
        valid_received_msgs_panel,
        processed_msgs_panel,
        number_of_connections_panel,
    ]
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
    panels.extend(get_unlabeled_local_client_panels(metrics.get_local_client_metrics()));
    panels.extend(get_unlabeled_remote_client_panels(metrics.get_remote_client_metrics()));
    panels.extend(get_unlabeled_local_server_panels(metrics.get_local_server_metrics()));
    panels.extend(get_unlabeled_remote_server_panels(metrics.get_remote_server_metrics()));
    panels.extend(labeled_client_panels);
    panels.extend(labeled_server_panels);

    Row::new(row_name, panels)
}
