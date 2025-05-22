use std::collections::HashMap;

use apollo_metrics::metrics::{MetricCounter, MetricGauge, MetricHistogram};
use indexmap::IndexMap;
use serde::ser::{SerializeMap, SerializeStruct};
use serde::{Serialize, Serializer};

#[cfg(test)]
#[path = "dashboard_test.rs"]
mod dashboard_test;

/// Grafana panel types.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub enum PanelType {
    #[serde(rename = "stat")]
    Stat,
    #[serde(rename = "graph")]
    Graph,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    name: &'static str,
    description: &'static str,
    expr: &'static str,
    panel_type: PanelType,
}

impl Panel {
    pub const fn new(
        name: &'static str,
        description: &'static str,
        expr: &'static str,
        panel_type: PanelType,
    ) -> Self {
        Self { name, description, expr, panel_type }
    }

    pub const fn from_counter(metric: MetricCounter, panel_type: PanelType) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            metric.get_name_with_filter(),
            panel_type,
        )
    }

    pub const fn from_gauge(metric: MetricGauge, panel_type: PanelType) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            metric.get_name_with_filter(),
            panel_type,
        )
    }

    pub const fn from_hist(metric: MetricHistogram, panel_type: PanelType) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            metric.get_name_with_filter(),
            panel_type,
        )
    }
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
        state.serialize_field("expr", &self.expr)?;

        // Append an empty dictionary `{}` at the end
        let empty_map: HashMap<String, String> = HashMap::new();
        state.serialize_field("extra_params", &empty_map)?;

        state.end()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    name: &'static str,
    panels: &'static [Panel],
}

impl Row {
    pub const fn new(name: &'static str, panels: &'static [Panel]) -> Self {
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

#[derive(Clone, Debug, PartialEq)]
pub struct Dashboard {
    name: &'static str,
    rows: &'static [Row],
}

impl Dashboard {
    pub const fn new(name: &'static str, description: &'static str, rows: &'static [Row]) -> Self {
        // TODO(Tsabary): remove description.
        let _ = description;
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
        for row in self.rows {
            row_map.insert(row.name, row.panels);
        }

        map.serialize_entry(self.name, &row_map)?;
        map.end()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum AlertComparisonOp {
    #[serde(rename = "gt")]
    GreaterThan,
    #[serde(rename = "lt")]
    LessThan,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertLogicalOp {
    And,
    Or,
}

/// Defines the condition to trigger the alert.
#[derive(Clone, Debug, PartialEq)]
pub struct AlertCondition {
    // The comparison operator to use when comparing the expression to the value.
    pub comparison_op: AlertComparisonOp,
    // The value to compare the expression to.
    pub comparison_value: f64,
    // The logical operator between this condition and other conditions.
    // TODO(Yael): Consider moving this field to the be one per alert to avoid ambiguity when
    // trying to use a combination of `and` and `or` operators.
    pub logical_op: AlertLogicalOp,
}

impl Serialize for AlertCondition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AlertCondition", 4)?;

        state.serialize_field(
            "evaluator",
            &serde_json::json!({
                "params": [self.comparison_value],
                "type": self.comparison_op
            }),
        )?;

        state.serialize_field(
            "operator",
            &serde_json::json!({
                "type": self.logical_op
            }),
        )?;

        state.serialize_field(
            "reducer",
            &serde_json::json!({
                "params": [],
                "type": "avg"
            }),
        )?;

        state.serialize_field("type", "query")?;

        state.end()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertGroup {
    Batcher,
    Gateway,
    HttpServer,
    Mempool,
}

/// Describes the properties of an alert defined in grafana.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Alert {
    // The name of the alert.
    pub name: &'static str,
    // The title that will be displayed.
    pub title: &'static str,
    // The group that the alert will be displayed under.
    #[serde(rename = "ruleGroup")]
    pub alert_group: AlertGroup,
    // The expression to evaluate for the alert.
    pub expr: &'static str,
    // The conditions that must be met for the alert to be triggered.
    pub conditions: &'static [AlertCondition],
    // The time duration for which the alert conditions must be true before an alert is triggered.
    #[serde(rename = "for")]
    pub pending_duration: &'static str,
    // The interval in sec between evaluations of the alert.
    #[serde(rename = "intervalSec")]
    pub evaluation_interval_sec: u64,
}

/// Description of the alerts to be configured in the dashboard.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Alerts {
    alerts: &'static [Alert],
}

impl Alerts {
    pub const fn new(alerts: &'static [Alert]) -> Self {
        Self { alerts }
    }
}
