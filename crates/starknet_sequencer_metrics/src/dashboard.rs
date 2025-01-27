use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

/// Grafana panel types.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum PanelType {
    Stat,
    Graph,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub struct Row<'a> {
    name: &'static str,
    description: &'static str,
    panels: &'a [Panel],
}

impl<'a> Row<'a> {
    pub const fn new(name: &'static str, description: &'static str, panels: &'a [Panel]) -> Self {
        Self { name, description, panels }
    }
}

// Custom Serialize implementation for Row
impl Serialize for Row<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Row", 3)?;
        state.serialize_field("name", self.name)?;
        state.serialize_field("description", self.description)?;

        // Add position for each panel
        let panels_with_positions: Vec<_> = self
            .panels
            .iter()
            .enumerate()
            .map(|(index, panel)| {
                let mut panel_with_position = serde_json::to_value(panel).unwrap();
                panel_with_position
                    .as_object_mut()
                    .unwrap()
                    .insert("position".to_string(), serde_json::json!(index + 1));
                panel_with_position
            })
            .collect();

        state.serialize_field("panels", &panels_with_positions)?;
        state.end()
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Dashboard<'a> {
    name: &'static str,
    description: &'static str,
    rows: &'a [Row<'a>],
}

impl<'a> Dashboard<'a> {
    pub const fn new(name: &'static str, description: &'static str, rows: &'a [Row<'a>]) -> Self {
        Self { name, description, rows }
    }
}
