use blockifier::metrics::{
    CLASS_CACHE_HITS,
    CLASS_CACHE_MISSES,
    NATIVE_CLASS_RETURNED,
    NATIVE_COMPILATION_ERROR,
    STATE_READER_METRIC_RATE_DURATION,
};

use crate::dashboard::{Panel, PanelType, Row};

// TODO(MatanL/Shahak): use clamp_min(X, 1) on denom to avoid division by zero.
fn get_panel_blockifier_state_reader_class_cache_miss_ratio() -> Panel {
    Panel::new(
        "class_cache_miss_ratio",
        "The ratio of cache misses when requesting compiled classes from the Blockifier State \
         Reader",
        vec![format!(
            "100 * (increase({}[{}]) / (increase({}[{}]) + increase({}[{}])))",
            CLASS_CACHE_MISSES.get_name_with_filter(),
            STATE_READER_METRIC_RATE_DURATION,
            CLASS_CACHE_MISSES.get_name_with_filter(),
            STATE_READER_METRIC_RATE_DURATION,
            CLASS_CACHE_HITS.get_name_with_filter(),
            STATE_READER_METRIC_RATE_DURATION
        )],
        PanelType::TimeSeries,
    )
}

// TODO(MatanL/Shahak): use clamp_min(X, 1) on denom to avoid division by zero.
fn get_panel_blockifier_state_reader_native_class_returned_ratio() -> Panel {
    Panel::new(
        "native_class_returned_ratio",
        "The ratio of Native classes returned by the Blockifier State Reader",
        vec![format!(
            "100 * (increase({}[{}]) / (increase({}[{}]) + increase({}[{}])))",
            NATIVE_CLASS_RETURNED.get_name_with_filter(),
            STATE_READER_METRIC_RATE_DURATION,
            CLASS_CACHE_HITS.get_name_with_filter(),
            STATE_READER_METRIC_RATE_DURATION,
            CLASS_CACHE_MISSES.get_name_with_filter(),
            STATE_READER_METRIC_RATE_DURATION,
        )],
        PanelType::TimeSeries,
    )
}

fn get_panel_native_compilation_error() -> Panel {
    Panel::from_counter(NATIVE_COMPILATION_ERROR, PanelType::Stat)
}

pub(crate) fn get_blockifier_state_reader_row() -> Row {
    Row::new(
        "Blockifier State Reader",
        vec![
            get_panel_blockifier_state_reader_class_cache_miss_ratio(),
            get_panel_blockifier_state_reader_native_class_returned_ratio(),
            get_panel_native_compilation_error(),
        ],
    )
}
