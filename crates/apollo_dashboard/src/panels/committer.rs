use apollo_committer::metrics::{
    AVERAGE_COMPUTE_RATE,
    AVERAGE_READ_RATE,
    AVERAGE_WRITE_RATE,
    BLOCKS_COMMITTED,
    COMMITTER_OFFSET,
    COMPUTE_DURATION_PER_BLOCK,
    COUNT_CLASSES_TRIE_MODIFICATIONS_PER_BLOCK,
    COUNT_CONTRACTS_TRIE_MODIFICATIONS_PER_BLOCK,
    COUNT_EMPTIED_LEAVES_PER_BLOCK,
    COUNT_STORAGE_TRIES_MODIFICATIONS_PER_BLOCK,
    EMPTIED_LEAVES_PERCENTAGE_PER_BLOCK,
    READ_DURATION_PER_BLOCK,
    TOTAL_BLOCK_DURATION,
    TOTAL_BLOCK_DURATION_PER_MODIFICATION,
    WRITE_DURATION_PER_BLOCK,
};
use apollo_metrics::metrics::MetricQueryName;

use crate::dashboard::Row;
use crate::panel::{Panel, PanelType, Unit};
use crate::query_builder::increase;

const BLOCK_DURATIONS_LOG_QUERY: &str = "\"Block\" AND \"durations in ms\"";
const RATES_LOG_QUERY: &str = "\"Block\" AND \"rates\"";
const COUNT_MODIFICATIONS_LOG_QUERY: &str = "\"Block\" AND \"modifications count\"";

fn get_offset_panel() -> Panel {
    Panel::new(
        "Committer Offset",
        "The next block number to commit",
        COMMITTER_OFFSET.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}

/// Returns a panel that shows the average of a counter per block over a 1m window.
fn average_per_block_panel(
    name: impl ToString,
    description: impl ToString,
    numerator: &dyn MetricQueryName,
    divisor: Option<u64>,
    log_query: Option<&str>,
    unit: Unit,
) -> Panel {
    let blocks = increase(&BLOCKS_COMMITTED, "1m");
    let divisor = match divisor {
        Some(n) => format!("{} * {}", n, blocks),
        None => blocks,
    };
    let expr = format!("{} / clamp_min({}, 1)", increase(numerator, "1m"), divisor,);
    let mut panel = Panel::new(name, description, expr, PanelType::TimeSeries).with_unit(unit);
    if let Some(q) = log_query {
        panel = panel.with_log_query(q);
    }
    panel
}

fn get_total_block_duration_panel() -> Panel {
    // Divide by 1000 to display in seconds.
    average_per_block_panel(
        "Total Block Duration",
        "Average total block duration over a 1m window",
        &TOTAL_BLOCK_DURATION,
        Some(1000),
        Some(BLOCK_DURATIONS_LOG_QUERY),
        Unit::Seconds,
    )
}

fn get_total_block_duration_per_modification_panel() -> Panel {
    average_per_block_panel(
        "Total Block Duration per Modification",
        "Average total block duration per modification over a 1m window",
        &TOTAL_BLOCK_DURATION_PER_MODIFICATION,
        Some(1_000_000),
        Some("total block duration per modification"),
        Unit::Seconds,
    )
}

fn get_read_duration_per_block_panel() -> Panel {
    // Divide by 1000 to display in seconds.
    average_per_block_panel(
        "Read Duration per Block",
        "Average read duration per block over a 1m window",
        &READ_DURATION_PER_BLOCK,
        Some(1000),
        Some(BLOCK_DURATIONS_LOG_QUERY),
        Unit::Seconds,
    )
}

fn get_compute_duration_per_block_panel() -> Panel {
    average_per_block_panel(
        "Compute Duration per Block",
        "Average compute duration per block over a 1m window",
        &COMPUTE_DURATION_PER_BLOCK,
        Some(1000),
        Some(BLOCK_DURATIONS_LOG_QUERY),
        Unit::Seconds,
    )
}

fn get_write_duration_per_block_panel() -> Panel {
    // Divide by 1000 to display in milliseconds.
    average_per_block_panel(
        "Write Duration per Block",
        "Average write duration per block over a 1m window",
        &WRITE_DURATION_PER_BLOCK,
        Some(1000),
        Some(BLOCK_DURATIONS_LOG_QUERY),
        Unit::Seconds,
    )
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
