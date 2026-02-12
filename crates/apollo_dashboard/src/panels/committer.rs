use apollo_committer::metrics::{
    AVERAGE_COMPUTE_RATE,
    AVERAGE_READ_RATE,
    AVERAGE_WRITE_RATE,
    COMPUTE_DURATION_PER_BLOCK,
    COUNT_CLASSES_TRIE_MODIFICATIONS_PER_BLOCK,
    COUNT_CONTRACTS_TRIE_MODIFICATIONS_PER_BLOCK,
    COUNT_EMPTIED_LEAVES_PER_BLOCK,
    COUNT_STORAGE_TRIES_MODIFICATIONS_PER_BLOCK,
    EMPTIED_LEAVES_PERCENTAGE_PER_BLOCK,
    OFFSET,
    READ_DURATION_PER_BLOCK,
    TOTAL_BLOCK_DURATION,
    TOTAL_BLOCK_DURATION_PER_MODIFICATION,
    WRITE_DURATION_PER_BLOCK,
};
use apollo_metrics::metrics::MetricQueryName;

use crate::dashboard::Row;
use crate::panel::{Panel, PanelType, Unit};

const BLOCK_DURATIONS_LOG_QUERY: &str = "\"Block\" AND \"durations in ms\"";
const RATES_LOG_QUERY: &str = "\"Block\" AND \"rates\"";
const COUNT_MODIFICATIONS_LOG_QUERY: &str = "\"Block\" AND \"modifications count\"";

fn get_offset_panel() -> Panel {
    Panel::new(
        "Offset",
        "The next block number to commit",
        OFFSET.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}

fn get_total_block_duration_panel() -> Panel {
    Panel::from_hist(&TOTAL_BLOCK_DURATION, "Total Block Duration", "Total block duration")
        .with_unit(Unit::Seconds)
        .with_log_query(BLOCK_DURATIONS_LOG_QUERY)
}

fn get_total_block_duration_per_modification_panel() -> Panel {
    Panel::from_hist(
        &TOTAL_BLOCK_DURATION_PER_MODIFICATION,
        "Total Block Duration per Modification",
        "Total block duration normalized by the number of modifications",
    )
    .with_unit(Unit::Seconds)
    .with_log_query(BLOCK_DURATIONS_LOG_QUERY)
}

fn get_read_duration_per_block_panel() -> Panel {
    Panel::from_hist(&READ_DURATION_PER_BLOCK, "Read Duration per Block", "Read duration per block")
        .with_unit(Unit::Seconds)
        .with_log_query(BLOCK_DURATIONS_LOG_QUERY)
}

fn get_compute_duration_per_block_panel() -> Panel {
    Panel::from_hist(
        &COMPUTE_DURATION_PER_BLOCK,
        "Compute Duration per Block",
        "Compute duration per block",
    )
    .with_unit(Unit::Seconds)
    .with_log_query(BLOCK_DURATIONS_LOG_QUERY)
}

fn get_write_duration_per_block_panel() -> Panel {
    Panel::from_hist(
        &WRITE_DURATION_PER_BLOCK,
        "Write Duration per Block",
        "Write duration per block",
    )
    .with_unit(Unit::Seconds)
    .with_log_query(BLOCK_DURATIONS_LOG_QUERY)
}

fn get_average_read_rate_panel() -> Panel {
    Panel::from_hist(
        &AVERAGE_READ_RATE,
        "Average Read Rate (entries/sec)",
        "Average read rate over a block",
    )
    .with_log_query(RATES_LOG_QUERY)
}

fn get_average_compute_rate_panel() -> Panel {
    Panel::from_hist(
        &AVERAGE_COMPUTE_RATE,
        "Average Compute Rate (entries/sec)",
        "Average compute rate over a block",
    )
    .with_log_query(RATES_LOG_QUERY)
}

fn get_average_write_rate_panel() -> Panel {
    Panel::from_hist(
        &AVERAGE_WRITE_RATE,
        "Average Write Rate (entries/sec)",
        "Average write rate over a block",
    )
    .with_log_query(RATES_LOG_QUERY)
}

fn get_count_storage_tries_modifications_per_block_panel() -> Panel {
    Panel::from_hist(
        &COUNT_STORAGE_TRIES_MODIFICATIONS_PER_BLOCK,
        "Storage Tries Modifications per Block",
        "Count of storage tries modifications per block",
    )
    .with_log_query(COUNT_MODIFICATIONS_LOG_QUERY)
}

fn get_count_contracts_trie_modifications_per_block_panel() -> Panel {
    Panel::from_hist(
        &COUNT_CONTRACTS_TRIE_MODIFICATIONS_PER_BLOCK,
        "Contracts Trie Modifications per Block",
        "Count of contracts trie modifications per block",
    )
    .with_log_query(COUNT_MODIFICATIONS_LOG_QUERY)
}

fn get_count_classes_trie_modifications_per_block_panel() -> Panel {
    Panel::from_hist(
        &COUNT_CLASSES_TRIE_MODIFICATIONS_PER_BLOCK,
        "Classes Trie Modifications per Block",
        "Count of classes trie modifications per block",
    )
    .with_log_query(COUNT_MODIFICATIONS_LOG_QUERY)
}

fn get_count_emptied_leaves_per_block_panel() -> Panel {
    Panel::from_hist(
        &COUNT_EMPTIED_LEAVES_PER_BLOCK,
        "Emptied Leaves per Block",
        "Count of storage tries leaves emptied per block",
    )
    .with_log_query(COUNT_MODIFICATIONS_LOG_QUERY)
}

fn get_percentage_emptied_leaves_per_block_panel() -> Panel {
    Panel::from_hist(
        &EMPTIED_LEAVES_PERCENTAGE_PER_BLOCK,
        "Percentage Emptied Leaves per Block",
        "Percentage of storage tries leaves emptied over the total number of storage tries leaves \
         per block",
    )
    .with_log_query(COUNT_MODIFICATIONS_LOG_QUERY)
}

pub(crate) fn get_committer_row() -> Row {
    Row::new(
        "Committer",
        vec![
            get_offset_panel(),
            get_total_block_duration_panel(),
            get_total_block_duration_per_modification_panel(),
            get_read_duration_per_block_panel(),
            get_average_read_rate_panel(),
            get_compute_duration_per_block_panel(),
            get_average_compute_rate_panel(),
            get_write_duration_per_block_panel(),
            get_average_write_rate_panel(),
            get_count_storage_tries_modifications_per_block_panel(),
            get_count_contracts_trie_modifications_per_block_panel(),
            get_count_classes_trie_modifications_per_block_panel(),
            get_count_emptied_leaves_per_block_panel(),
            get_percentage_emptied_leaves_per_block_panel(),
        ],
    )
}
