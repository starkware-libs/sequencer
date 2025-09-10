use apollo_compile_to_casm::metrics::COMPILATION_DURATION;

use crate::dashboard::{Panel, Row};

fn get_panel_compilation_duration() -> Panel {
    Panel::from(&COMPILATION_DURATION)
}

pub(crate) fn get_compile_to_casm_row() -> Row {
    Row::new("Compile sierra to casm", vec![get_panel_compilation_duration()])
}
