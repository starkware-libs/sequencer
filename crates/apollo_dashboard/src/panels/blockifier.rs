use apollo_batcher::metrics::{
    NUM_TRANSACTION_IN_BLOCK,
    PROVING_GAS_IN_LAST_BLOCK,
    SIERRA_GAS_IN_LAST_BLOCK,
};
use apollo_metrics::metrics::MetricQueryName;
use blockifier::metrics::{
    BLOCKIFIER_METRIC_RATE_DURATION,
    CALLS_RUNNING_NATIVE,
    CLASS_CACHE_HITS,
    CLASS_CACHE_MISSES,
    NATIVE_CLASS_RETURNED,
    NATIVE_COMPILATION_ERROR,
    TOTAL_CALLS,
};

use crate::dashboard::{Panel, PanelType, Row};

const DENOMINATOR_DIVISOR_FOR_READABILITY: f64 = 1_000_000_000.0;

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
    Panel::new(
        "Native compilation error count",
        "Count of the number of times there was a native compilation error",
        NATIVE_COMPILATION_ERROR.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
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

fn get_panel_transactions_per_block() -> Panel {
    Panel::from_hist(
        &NUM_TRANSACTION_IN_BLOCK,
        "Transactions Per Block",
        "The number of transactions per block",
    )
}

fn get_panel_sierra_gas_in_last_block() -> Panel {
    Panel::new(
        "Average Sierra Gas Usage in Block",
        "The average sierra gas usage in block (10m window)",
        format!(
            "avg_over_time({}[10m])/{}",
            SIERRA_GAS_IN_LAST_BLOCK.get_name_with_filter(),
            DENOMINATOR_DIVISOR_FOR_READABILITY
        ),
        PanelType::TimeSeries,
    )
}

fn get_panel_proving_gas_in_last_block() -> Panel {
    Panel::new(
        "Average Proving Gas Usage in Block",
        "The average proving gas usage in block (10m window)",
        format!(
            "avg_over_time({}[10m])/{}",
            PROVING_GAS_IN_LAST_BLOCK.get_name_with_filter(),
            DENOMINATOR_DIVISOR_FOR_READABILITY
        ),
        PanelType::TimeSeries,
    )
}

pub(crate) fn get_blockifier_row() -> Row {
    Row::new(
        "Blockifier",
        vec![
            get_panel_blockifier_state_reader_class_cache_miss_ratio(),
            get_panel_blockifier_state_reader_native_class_returned_ratio(),
            get_panel_native_compilation_error(),
            get_panel_native_execution_ratio(),
            get_panel_transactions_per_block(),
            get_panel_sierra_gas_in_last_block(),
            get_panel_proving_gas_in_last_block(),
        ],
    )
}
