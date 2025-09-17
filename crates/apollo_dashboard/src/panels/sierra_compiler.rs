use apollo_class_manager::metrics::{CLASS_SIZES, N_CLASSES};
use apollo_compile_to_casm::metrics::COMPILATION_DURATION;

use crate::dashboard::{Panel, PanelType, Row, Unit};

fn get_panel_compilation_duration() -> Panel {
    Panel::from_hist(
        &COMPILATION_DURATION,
        "Compile to Casm Compilation Duration",
        "Server-side compilation of Sierra to Casm duration",
    )
    .with_unit(Unit::Seconds)
}
fn get_panel_n_classes() -> Panel {
    Panel::new(
        "Number of Classes",
        "Number of classes, labeled by type (regular, deprecated)",
        vec![format!(
            "sum by ({}) (increase({}[10m]))",
            "class_type",
            N_CLASSES.get_name_with_filter()
        )],
        PanelType::Stat,
    )
}
fn get_panel_class_sizes() -> Panel {
    Panel::from_labeled_hist(
        &CLASS_SIZES,
        "Class Sizes",
        "Size of the classes in bytes, labeled by type (sierra, casm, deprecated casm)",
    )
    .with_unit(Unit::MB)
}

pub(crate) fn get_compile_to_casm_row() -> Row {
    Row::new(
        "Class Manager",
        vec![get_panel_compilation_duration(), get_panel_n_classes(), get_panel_class_sizes()],
    )
}
