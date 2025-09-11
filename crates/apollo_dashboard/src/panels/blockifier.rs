use blockifier::metrics::{
    BLOCKIFIER_METRIC_RATE_DURATION,
    CALLS_RUNNING_NATIVE,
    CLASS_CACHE_HITS,
    CLASS_CACHE_MISSES,
    NATIVE_CLASS_RETURNED,
    NATIVE_COMPILATION_ERROR,
    N_BLOCKS,
    N_MIGRATIONS,
    TOTAL_CALLS,
};

use crate::dashboard::{Panel, PanelType, Row};

// TODO(MatanL/Shahak): use clamp_min(X, 1) on denom to avoid division by zero.
fn get_panel_blockifier_state_reader_class_cache_miss_ratio() -> Panel {
    Panel::ratio_time_series(
        "class_cache_miss_ratio",
        "The ratio of cache misses when requesting compiled classes from the Blockifier State \
         Reader",
        &CLASS_CACHE_MISSES,
        &[&CLASS_CACHE_MISSES, &CLASS_CACHE_HITS],
        BLOCKIFIER_METRIC_RATE_DURATION,
    )
}

// TODO(MatanL/Shahak): use clamp_min(X, 1) on denom to avoid division by zero.
fn get_panel_blockifier_state_reader_native_class_returned_ratio() -> Panel {
    Panel::ratio_time_series(
        "native_class_returned_ratio",
        "The ratio of Native classes returned by the Blockifier",
        &NATIVE_CLASS_RETURNED,
        &[&CLASS_CACHE_HITS, &CLASS_CACHE_MISSES],
        BLOCKIFIER_METRIC_RATE_DURATION,
    )
}

fn get_panel_native_compilation_error() -> Panel {
    Panel::from(&NATIVE_COMPILATION_ERROR)
}

fn get_panel_native_execution_ratio() -> Panel {
    Panel::ratio_time_series(
        "native_execution_ratio",
        "The ratio of calls running Cairo Native in the Blockifier",
        &CALLS_RUNNING_NATIVE,
        &[&TOTAL_CALLS],
        BLOCKIFIER_METRIC_RATE_DURATION,
    )
}

fn get_panel_number_of_migrations() -> Panel {
    Panel::from(&N_MIGRATIONS)
}
fn get_panel_avg_migrations_per_block() -> Panel {
    let name = "avg_migrations_per_block";
    let description = "The average number of state migrations performed per block";
    let numerator_expr = format!(
        "increase({}[{}])",
        N_MIGRATIONS.get_name_with_filter(),
        BLOCKIFIER_METRIC_RATE_DURATION
    );
    let denominator_expr = format!(
        "increase({}[{}])",
        N_BLOCKS.get_name_with_filter(),
        BLOCKIFIER_METRIC_RATE_DURATION
    );
    let expression = format!("{} / {}", numerator_expr, denominator_expr);
    Panel::new(name, description, vec![expression], PanelType::TimeSeries)
}

pub(crate) fn get_blockifier_row() -> Row {
    Row::new(
        "Blockifier",
        vec![
            get_panel_blockifier_state_reader_class_cache_miss_ratio(),
            get_panel_blockifier_state_reader_native_class_returned_ratio(),
            get_panel_native_compilation_error(),
            get_panel_native_execution_ratio(),
            get_panel_number_of_migrations(),
            get_panel_avg_migrations_per_block(),
        ],
    )
}
