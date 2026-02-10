use indexmap::IndexMap;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

use crate::panel::Panel;

#[cfg(test)]
#[path = "dashboard_test.rs"]
mod dashboard_test;

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
