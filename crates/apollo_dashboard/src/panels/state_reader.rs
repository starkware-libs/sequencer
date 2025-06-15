use blockifier::metrics::{
    CLASS_CACHE_HITS,
    CLASS_CACHE_MISSES,
    NATIVE_CLASS_RETURNED,
    NATIVE_COMPILATION_ERROR,
    STATE_READER_METRIC_RATE_DURATION,
};
use const_format::formatcp;

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_BLOCKIFIER_STATE_READER_CLASS_CACHE_MISS_RATIO: Panel = Panel::new(
    "class_cache_miss_ratio",
    "The ratio of cache misses when requesting compiled classes from the Blockifier State Reader",
    formatcp!(
        "100 * (increase({}[{}]) / (increase({}[{}]) + increase({}[{}])))",
        CLASS_CACHE_MISSES.get_name_with_filter(),
        STATE_READER_METRIC_RATE_DURATION,
        CLASS_CACHE_MISSES.get_name_with_filter(),
        STATE_READER_METRIC_RATE_DURATION,
        CLASS_CACHE_HITS.get_name_with_filter(),
        STATE_READER_METRIC_RATE_DURATION
    ),
    PanelType::Graph,
);

pub(crate) const PANEL_BLOCKIFIER_STATE_READER_NATIVE_CLASS_RETURNED_RATIO: Panel = Panel::new(
    "native_class_returned_ratio",
    "The ratio of Native classes returned by the Blockifier State Reader",
    formatcp!(
        "100 * (increase({}[{}]) / (increase({}[{}]) + increase({}[{}])))",
        NATIVE_CLASS_RETURNED.get_name_with_filter(),
        STATE_READER_METRIC_RATE_DURATION,
        CLASS_CACHE_HITS.get_name_with_filter(),
        STATE_READER_METRIC_RATE_DURATION,
        CLASS_CACHE_MISSES.get_name_with_filter(),
        STATE_READER_METRIC_RATE_DURATION,
    ),
    PanelType::Graph,
);

pub(crate) const PANEL_NATIVE_COMPILATION_ERROR: Panel =
    Panel::from_counter(NATIVE_COMPILATION_ERROR, PanelType::Stat);
