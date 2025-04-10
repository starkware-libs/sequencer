use apollo_state_reader::metrics::{CLASS_CACHE_HITS, CLASS_CACHE_MISSES, NATIVE_CLASS_RETURNED};
use const_format::formatcp;

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_APOLLO_STATE_READER_CLASS_CACHE_MISS_RATIO: Panel = Panel::new(
    "class_cache_miss_ratio",
    "The ratio of cache misses when requesting compiled classes from the apollo state reader",
    formatcp!(
        "100 * ({} / max(({} + {}), 1))",
        CLASS_CACHE_MISSES.get_name(),
        CLASS_CACHE_MISSES.get_name(),
        CLASS_CACHE_HITS.get_name()
    ),
    PanelType::Graph,
);
pub(crate) const PANEL_APOLLO_STATE_READER_NATIVE_CLASS_RETURNED_RATIO: Panel = Panel::new(
    "native_class_returned_ratio",
    "The ratio of Native classes returned by the apollo state reader",
    formatcp!(
        "100 * ({} / max(({} + {}), 1))",
        NATIVE_CLASS_RETURNED.get_name(),
        CLASS_CACHE_HITS.get_name(),
        CLASS_CACHE_MISSES.get_name()
    ),
    PanelType::Graph,
);
