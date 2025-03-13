use std::collections::HashMap;

use indexmap::IndexMap;
use serde::ser::{SerializeMap, SerializeStruct};
use serde::{Serialize, Serializer};

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
pub struct Row<'a> {
    name: &'static str,
    panels: &'a [Panel],
}

impl<'a> Row<'a> {
    pub const fn new(name: &'static str, description: &'static str, panels: &'a [Panel]) -> Self {
        // TODO(Tsabary): remove description.
        let _ = description;
        Self { name, panels }
    }
}

// Custom Serialize implementation for Row.
impl Serialize for Row<'_> {
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
pub struct Dashboard<'a> {
    name: &'static str,
    rows: &'a [Row<'a>],
}

impl<'a> Dashboard<'a> {
    pub const fn new(name: &'static str, description: &'static str, rows: &'a [Row<'a>]) -> Self {
        // TODO(Tsabary): remove description.
        let _ = description;
        Self { name, rows }
    }
}

// Custom Serialize implementation for Dashboard.
impl Serialize for Dashboard<'_> {
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
pub enum ComparisonOp {
    #[serde(rename = "gt")]
    GreaterThan,
    #[serde(rename = "lt")]
    LessThan,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum LogicalOp {
    #[serde(rename = "and")]
    And,
    #[serde(rename = "or")]
    Or,
}

/// Defines the condition to trigger the alert.
#[derive(Clone, Debug, PartialEq)]
pub struct AlertCondition {
    // The expression to evaluate for the alert.
    expr: &'static str,
    // The comparison operator to use when comparing the expression to the value.
    comparison_op: ComparisonOp,
    // The value to compare the expression to.
    comparison_value: f64,
    // The logical operator to operate between conditions.
    logical_op: LogicalOp,
}

impl Serialize for AlertCondition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AlertCondition", 2)?;

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
            "query",
            &serde_json::json!({
                "expr": self.expr
            }),
        )?;

        state.end()
    }
}

/// Describes the properties of an alert defined in grafana.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Alert {
    // The required duration for which the conditions must remain true before triggering the alert.
    name: &'static str,
    // The message that will be displayed or sent when the alert is triggered.
    message: &'static str,
    // The conditions that must be met for the alert to be triggered.
    conditions: &'static [AlertCondition],
    // The time duration for which the alert conditions must be true before an alert is triggered.
    #[serde(rename = "for")]
    pending_duration: &'static str,
}
