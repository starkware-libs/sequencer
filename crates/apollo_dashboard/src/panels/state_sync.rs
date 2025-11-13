use apollo_metrics::MetricCommon;
use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    STATE_SYNC_BODY_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
    STATE_SYNC_HEADER_LATENCY_SEC,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};

fn get_panel_central_sync_central_block_marker() -> Panel {
    Panel::new(
        "Central Block Marker",
        "The first block that Central Starknet hasn't seen yet",
        CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}
fn get_panel_state_sync_body_marker() -> Panel {
    Panel::new(
        "State Sync Body Marker",
        "The first block number for which the state sync component does not have a body",
        STATE_SYNC_BODY_MARKER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}
fn get_panel_state_sync_diff_from_central() -> Panel {
    Panel::new(
        "Sync Diff From Central",
        "The number of blocks that were not fully synced yet",
        format!(
            "{} - {}",
            CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter(),
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
        ),
        PanelType::TimeSeries,
    )
}
fn get_panel_state_sync_new_header_maturity() -> Panel {
    Panel::new(
        "Sync Block Age",
        "The time from a block’s timestamp until its header is synced through the feeder-gateway.",
        STATE_SYNC_HEADER_LATENCY_SEC.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}

fn get_panel_state_sync_eta() -> Panel {
    let central_marker = CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter();
    let state_sync_marker = STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter();
    let diff = format!("{} - {}", central_marker, state_sync_marker);

    // Check if diff was 0 within the last minute
    let was_equal_recently = format!("max_over_time(({} == 0)[1m:])", diff);

    // The gap closes at: rate(STATE_SYNC) - rate(CENTRAL) which is equivalent to: -rate(diff)
    let gap_closing_rate = format!("-rate(({})[5m])", diff);
    let eta = format!("({} / clamp_min({}, 0.0001))", diff, gap_closing_rate);

    // If diff was 0 within the last minute, show 0 (synced)
    // If gap is closing (gap_closing_rate > 0), show ETA
    // Otherwise (gap growing or not closing), show nothing (NaN)
    let expr =
        format!("(({} == 1) * 0) or (({} > 0) * {})", was_equal_recently, gap_closing_rate, eta);

    Panel::new("Sync ETA", "Estimated time until sync finishes.", expr, PanelType::Stat)
        .with_unit(Unit::Seconds)
}

pub(crate) fn get_state_sync_row() -> Row {
    Row::new(
        "State Sync",
        vec![
            get_panel_central_sync_central_block_marker(),
            get_panel_state_sync_body_marker(),
            get_panel_state_sync_diff_from_central(),
            get_panel_state_sync_new_header_maturity(),
            get_panel_state_sync_eta(),
        ],
    )
}
