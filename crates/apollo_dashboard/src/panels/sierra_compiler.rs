use apollo_compile_to_casm::metrics::COMPILATION_DURATION;

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_compilation_duration() -> Panel {
    Panel::from_hist(&COMPILATION_DURATION, PanelType::TimeSeries)
}

pub(crate) fn get_compile_to_casm_row() -> Row {
    Row::new("Compile sierra to casm", vec![get_panel_compilation_duration()])
}
