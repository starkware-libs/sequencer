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
use serde_with::skip_serializing_none;

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

impl Serialize for Unit {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.grafana_id())
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

#[skip_serializing_none]
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct ExtraParams {
    pub unit: Option<Unit>,
    pub show_percent_change: Option<bool>,
    pub log_query: Option<String>,
    pub thresholds: Option<Thresholds>,
    pub legends: Option<Vec<String>>,
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
            "show_percent_change is only supported on Stat panels; got {:?}",
            self.panel_type
        );
        self.extra.show_percent_change = Some(true);
        self
    }
    #[allow(dead_code)] // TODO(Ron): use in panels
    pub fn with_log_query(mut self, log_query: impl Into<String>) -> Self {
        let mut query = log_query.into();

        // Add quotes if none are present.
        if !query.contains('"') {
            query = format!("\"{}\"", query);
        }

        self.extra.log_query = Some(query);
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

        let expr = format!("({} / ({}))", numerator_expr, denominator_expr);

        Self::new(name, description, vec![expr], PanelType::TimeSeries).with_unit(Unit::PercentUnit)
    }
}

#[allow(dead_code)] // TODO(Ron): use in panels
pub fn traffic_light_thresholds(yellow: f64, red: f64) -> Vec<(&'static str, Option<f64>)> {
    vec![("green", None), ("yellow", Some(yellow)), ("red", Some(red))]
}

pub(crate) fn create_request_type_labeled_hist_panels(
    metric: &LabeledMetricHistogram,
    panel_type: PanelType,
) -> Vec<Panel> {
    metric
        .get_flat_label_values()
        .into_iter()
        .map(|request_label| {
            Panel::from_request_type_labeled_hist(metric, panel_type, request_label)
        })
        .collect()
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
        state.serialize_field("extra_params", &self.extra)?;

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

pub(crate) fn get_local_client_panels(local_client_metrics: &LocalClientMetrics) -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        local_client_metrics.get_response_time_metric(),
        PanelType::TimeSeries,
    )
}

pub(crate) fn get_remote_client_panels(remote_client_metrics: &RemoteClientMetrics) -> Vec<Panel> {
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
    vec![attempts_panel]
        .into_iter()
        .chain(response_times_panels)
        .chain(communication_failure_times_panels)
        .collect()
}

pub(crate) fn get_local_server_panels(local_server_metrics: &LocalServerMetrics) -> Vec<Panel> {
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
    vec![received_msgs_panel, processed_msgs_panel, queue_depth_panel]
        .into_iter()
        .chain(processing_times_panels)
        .chain(queueing_times_panels)
        .collect()
}

pub(crate) fn get_remote_server_panels(remote_server_metrics: &RemoteServerMetrics) -> Vec<Panel> {
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
    .into_iter()
    .collect()
}

pub(crate) fn get_component_infra_row(row_name: &'static str, metrics: &InfraMetrics) -> Row {
    Row::new(
        row_name,
        vec![
            get_local_client_panels(metrics.get_local_client_metrics()),
            get_remote_client_panels(metrics.get_remote_client_metrics()),
            get_local_server_panels(metrics.get_local_server_metrics()),
            get_remote_server_panels(metrics.get_remote_server_metrics()),
        ]
        .into_iter()
        .flatten()
        .collect(),
    )
}
