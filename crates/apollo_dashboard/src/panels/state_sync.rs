use apollo_committer::metrics::COMMITTER_OFFSET;
use apollo_metrics::metrics::MetricQueryName;
use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    STATE_SYNC_BODY_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
    STATE_SYNC_HEADER_LATENCY_SEC,
};

use crate::dashboard::Row;
use crate::panel::{Panel, PanelType, Unit};
use crate::query_builder::DEFAULT_DURATION;

fn get_panel_current_feeder_gateway_marker() -> Panel {
    Panel::new(
        "Current Feeder Gateway Marker",
        "The latest block number available in the feeder gateway + 1",
        CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}
fn get_panel_sync_body_marker() -> Panel {
    Panel::new(
        "Current Sync Body Marker",
        "The first block whose body hasn't been downloaded",
        STATE_SYNC_BODY_MARKER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}
fn get_panel_current_sync_marker() -> Panel {
    Panel::new(
        "Current Sync Marker",
        "The first block that hasn't been fully downloaded",
        STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}
fn get_panel_sync_diff_from_feeder_gateway() -> Panel {
    Panel::new(
        "Sync Diff From Feeder Gateway",
        "The number of blocks that exist in the feeder but weren't downloaded yet",
        format!(
            "{} - {}",
            CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter(),
            STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter()
        ),
        PanelType::TimeSeries,
    )
}
fn get_panel_sync_block_age() -> Panel {
    Panel::new(
        "Sync Block Age",
        "The time from a block’s timestamp until its header is downloaded.",
        STATE_SYNC_HEADER_LATENCY_SEC.get_name_with_filter().to_string(),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}
fn get_panel_time_to_complete_sync() -> Panel {
    Panel::new(
        "Time to Complete Sync",
        format!(
            "Estimated time to complete syncing to the latest block (based on a {} window \
<<<<<<< HEAD
             rate).\nThe value is computed from the sync rate of the `current sync marker` \
             compared against the `current feeder gateway marker`.",
||||||| 8e2855c049
             rate).\nThe value is computed from the sync rate of the `class manager marker` \
             (which is the last component to finish downloading among all state sync parts), \
             compared against the `central block marker` (the latest block known to central).",
=======
             rate).\nThe value is computed from the sync rate of the `committer offset` (the next \
             block to commit, representing the current committed state), compared against the \
             `central block marker` (the latest block known to central).",
>>>>>>> origin/main-v0.14.1-committer
            DEFAULT_DURATION
        ),
        format!(
            "({target_total} - on (namespace) {committer_offset}) / \
             clamp_min(rate({committer_offset}[{d}]) - on (namespace) rate({target_total}[{d}]), \
             1)",
            target_total = CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter(),
            committer_offset = COMMITTER_OFFSET.get_name_with_filter(),
            d = DEFAULT_DURATION
        ),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}

pub(crate) fn get_state_sync_row() -> Row {
    Row::new(
        "State Sync",
        vec![
            get_panel_current_feeder_gateway_marker(),
            get_panel_current_sync_marker(),
            get_panel_time_to_complete_sync(),
            get_panel_sync_diff_from_feeder_gateway(),
            get_panel_sync_block_age(),
            get_panel_sync_body_marker(),
        ],
    )
}
