use crate::dashboard::{Dashboard, Panel, PanelType, Row};

const PANEL_EXAMPLE_1: Panel =
    Panel::new("row_example_1", "panel_default_description_1", "expr1", PanelType::Stat);
const PANEL_EXAMPLE_2: Panel =
    Panel::new("row_example_2", "panel_default_description_2", "expr2", PanelType::Stat);
const PANEL_EXAMPLE_3: Panel =
    Panel::new("row_example_3", "panel_default_description_3", "expr3", PanelType::Stat);
const PANEL_EXAMPLE_4: Panel =
    Panel::new("row_example_4", "panel_default_description_4", "expr4", PanelType::Stat);

const ROW_EXAMPLE_1: Row<'_> =
    Row::new("row_example_1", "row_default_description_1", &[PANEL_EXAMPLE_1, PANEL_EXAMPLE_2]);
const ROW_EXAMPLE_2: Row<'_> =
    Row::new("row_example_2", "row_default_description_2", &[PANEL_EXAMPLE_3, PANEL_EXAMPLE_4]);

pub const DASHBOARD_EXAMPLE: Dashboard<'_> = Dashboard::new(
    "dashboarad_example_1",
    "dashboarad_default_description_2",
    &[ROW_EXAMPLE_1, ROW_EXAMPLE_2],
);
