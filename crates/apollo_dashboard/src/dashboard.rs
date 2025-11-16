use apollo_metrics::metrics::{
    LabeledMetricHistogram,
    MetricCounter,
    MetricDetails,
    MetricGauge,
    MetricHistogram,
    MetricQueryName,
};
use indexmap::IndexMap;
use serde::ser::{SerializeMap, SerializeStruct};
use serde::{Serialize, Serializer};
use serde_with::skip_serializing_none;

use crate::query_builder;

#[cfg(test)]
#[path = "dashboard_test.rs"]
mod dashboard_test;

pub const HISTOGRAM_QUANTILES: &[f64] = &[0.50, 0.95];
pub const HISTOGRAM_TIME_RANGE: &str = "5m";

// TODO(Tsabary): split this entire module into sub modules.

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
        #[derive(Serialize)]
        struct RowValue<'a> {
            panels: &'a [Panel],
            collapsed: bool,
        }

        let mut map = serializer.serialize_map(Some(1))?;
        let mut row_map = IndexMap::new();
        for row in &self.rows {
            row_map.insert(
                row.name.clone(),
                RowValue { panels: &row.panels, collapsed: row.collapsed },
            );
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
    Bytes,
    Seconds,
    #[allow(clippy::enum_variant_names)]
    // The expected values for PercentUnit are [0,1]
    PercentUnit,
    MB,
    Microseconds,
}

impl Unit {
    fn grafana_id(&self) -> &'static str {
        match self {
            Unit::Bytes => "bytes",
            Unit::Seconds => "s",
            Unit::PercentUnit => "percentunit",
            Unit::MB => "decmbytes",
            Unit::Microseconds => "µs",
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
    pub log_comment: Option<String>,
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

// helper: unify "String" and "Vec<String>"
pub(crate) trait IntoExprs {
    fn into_exprs(self) -> Vec<String>;
}

impl IntoExprs for String {
    fn into_exprs(self) -> Vec<String> {
        vec![self]
    }
}

impl IntoExprs for &str {
    fn into_exprs(self) -> Vec<String> {
        vec![self.to_string()]
    }
}

impl IntoExprs for Vec<String> {
    fn into_exprs(self) -> Vec<String> {
        self
    }
}

impl IntoExprs for Vec<&str> {
    fn into_exprs(self) -> Vec<String> {
        self.into_iter().map(|s| s.to_string()).collect::<Vec<_>>()
    }
}

impl Panel {
    pub(crate) fn new(
        name: impl ToString,
        description: impl ToString,
        exprs: impl IntoExprs,
        panel_type: PanelType,
    ) -> Self {
        // A panel assigns a unique id to each of its expressions. Conventionally, we use letters
        // A–Z, and for simplicity, we limit the number of expressions to this range.
        const NUM_LETTERS: u8 = b'Z' - b'A' + 1;
        let name = name.to_string();
        let description = description.to_string();

        let exprs = exprs.into_exprs();

        assert!(
            exprs.len() <= NUM_LETTERS.into(),
            "Too many expressions ({} > {NUM_LETTERS}) in panel '{name}'.",
            exprs.len(),
        );
        Self { name, description, exprs, panel_type, extra: ExtraParams::default() }
    }

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

    fn quote_if_missing(s: &str) -> String {
        if s.contains('"') { s.to_string() } else { format!("\"{}\"", s) }
    }

    pub fn with_log_query(mut self, log_query: impl Into<String>) -> Self {
        let query = Self::quote_if_missing(&log_query.into());
        self.extra.log_query = Some(query);
        self
    }
    #[allow(dead_code)] // TODO(Ron): use in panels
    pub fn with_log_comment(mut self, log_comment: impl Into<String>) -> Self {
        let comment = Self::quote_if_missing(&log_comment.into());
        self.extra.log_comment = Some(format!("-- {}", comment));
        self
    }

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

    pub(crate) fn ratio_time_series(
        name: &'static str,
        description: &'static str,
        numerator: &dyn MetricQueryName,
        denominator_parts: &[&dyn MetricQueryName],
        duration: &str,
    ) -> Self {
        let numerator_expr = query_builder::increase(numerator, duration);

        let denominator_expr = denominator_parts
            .iter()
            .map(|m| query_builder::increase(*m, duration))
            .collect::<Vec<_>>()
            .join(" + ");

        let expr = format!("({numerator_expr} / ({denominator_expr}))");

        Self::new(name, description, expr, PanelType::TimeSeries).with_unit(Unit::PercentUnit)
    }

    pub(crate) fn from_counter(metric: &MetricCounter, panel_type: PanelType) -> Self {
        Self::new(
            metric_name_to_panel_title(metric.get_name()),
            metric.get_description(),
            metric.get_name_with_filter().to_string(),
            panel_type,
        )
    }

    pub(crate) fn from_gauge(metric: &MetricGauge, panel_type: PanelType) -> Self {
        Self::new(
            metric_name_to_panel_title(metric.get_name()),
            metric.get_description(),
            metric.get_name_with_filter().to_string(),
            panel_type,
        )
    }

    fn from_hist_helper(
        metric_name_with_filter: impl AsRef<str>,
        name: impl ToString,
        description: impl ToString,
        sum_by: impl AsRef<str>,
    ) -> Self {
        Self::new(
            metric_name_to_panel_title(name.to_string()),
            description.to_string(),
            HISTOGRAM_QUANTILES
                .iter()
                .map(|q| {
                    format!(
                        "histogram_quantile({q:.2}, sum by ({}) \
                         (rate({}[{HISTOGRAM_TIME_RANGE}])))",
                        sum_by.as_ref(),
                        metric_name_with_filter.as_ref(),
                    )
                })
                .collect::<Vec<_>>(),
            PanelType::TimeSeries,
        )
    }

    pub(crate) fn from_hist(
        metric: &MetricHistogram,
        name: impl ToString,
        description: impl ToString,
    ) -> Self {
        Self::from_hist_helper(metric.get_name_with_filter(), name, description, "le")
    }

    pub(crate) fn from_labeled_hist(
        metric: &LabeledMetricHistogram,
        name: impl ToString,
        description: impl ToString,
    ) -> Self {
        let group_label = metric.get_label_name();
        Self::from_hist_helper(
            metric.get_name_with_filter(),
            name,
            description,
            format!("le, {}", group_label),
        )
        .with_legends(
            HISTOGRAM_QUANTILES
                .iter()
                .map(|q| format!("{:.2} {{{{{}}}}}", q, group_label))
                .collect(),
        )
    }
}

// Creates a panel title from a metric name by formatting it in Title Case.
fn metric_name_to_panel_title(metric_name: impl ToString) -> String {
    fn title_case(s: String) -> String {
        s.split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().chain(chars).collect::<String>(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
    title_case(metric_name.to_string().replace("_", " "))
}

#[allow(dead_code)] // TODO(Ron): use in panels
pub fn traffic_light_thresholds(yellow: f64, red: f64) -> Vec<(&'static str, Option<f64>)> {
    vec![("green", None), ("yellow", Some(yellow)), ("red", Some(red))]
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
    name: String,
    panels: Vec<Panel>,
    collapsed: bool,
}

impl Row {
    pub(crate) fn new(name: impl ToString, panels: Vec<Panel>) -> Self {
        Self { name: name.to_string(), panels, collapsed: true }
    }
    pub fn expand(mut self) -> Self {
        self.collapsed = false;
        self
    }
}

// Custom Serialize implementation for Row.
impl Serialize for Row {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&self.name, &self.panels)?;
        map.end()
    }
}

/// Returns a PromQL expression that calculates the time since the last increase of the given
/// metric. Assumes there was an increase in the last 12 hours.
pub(crate) fn get_time_since_last_increase_expr(metric_name: &str) -> String {
    const TIME_RANGE: &str = "12h";
    format!(
        // The max over time is the timestamp of the last increase in the last 12 hours.
        "time() - max_over_time((timestamp(increase({metric_name}[{TIME_RANGE}])) != \
         0))[{TIME_RANGE}:])"
    )
}
