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
    pub const fn new(name: &'static str, rows: &'static [Row]) -> Self {
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
