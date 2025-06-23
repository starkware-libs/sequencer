use std::collections::HashMap;

use apollo_metrics::metrics::{MetricCounter, MetricGauge, MetricHistogram};
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
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PanelType {
    Stat,
    TimeSeries,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Panel {
    name: &'static str,
    description: &'static str,
    exprs: Vec<String>,
    panel_type: PanelType,
}

impl Panel {
    pub(crate) fn new(
        name: &'static str,
        description: &'static str,
        exprs: Vec<String>,
        panel_type: PanelType,
    ) -> Self {
        // A panel assigns a unique id to each of its expressions. Conventionally, we use letters
        // A–Z, and for simplicity, we limit the number of expressions to this range.
        const NUM_LETTERS: u8 = b'Z' - b'A' + 1;
        assert!(
            exprs.len() <= NUM_LETTERS.into(),
            "Too many expressions ({} > {}) in panel '{}'.",
            exprs.len(),
            NUM_LETTERS,
            name
        );
        Self { name, description, exprs, panel_type }
    }

    pub(crate) fn from_counter(metric: MetricCounter, panel_type: PanelType) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            vec![metric.get_name_with_filter().to_string()],
            panel_type,
        )
    }

    pub(crate) fn from_gauge(metric: MetricGauge, panel_type: PanelType) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            vec![metric.get_name_with_filter().to_string()],
            panel_type,
        )
    }

    pub(crate) fn from_hist(metric: MetricHistogram, panel_type: PanelType) -> Self {
        Self::new(
            metric.get_name(),
            metric.get_description(),
            HISTOGRAM_QUANTILES
                .iter()
                .map(|q| {
                    format!(
                        "histogram_quantile({:.2}, sum(rate({}[{}])) by (le))",
                        q,
                        metric.get_name_with_filter(),
                        HISTOGRAM_TIME_RANGE
                    )
                })
                .collect(),
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
