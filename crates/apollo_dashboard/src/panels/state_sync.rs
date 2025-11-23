<<<<<<< HEAD
use apollo_metrics::MetricCommon;
||||||| 912efc99a
=======
use apollo_metrics::metrics::MetricQueryName;
>>>>>>> origin/main-v0.14.1
use apollo_state_sync_metrics::metrics::{
    CENTRAL_SYNC_CENTRAL_BLOCK_MARKER,
    STATE_SYNC_BODY_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
    STATE_SYNC_COMPILED_CLASS_MARKER,
    STATE_SYNC_HEADER_LATENCY_SEC,
};

use crate::dashboard::{Panel, PanelType, Row, Unit};
use crate::query_builder::DEFAULT_DURATION;

fn get_panel_central_sync_central_block_marker() -> Panel {
    Panel::new(
        "Central Block Marker",
<<<<<<< HEAD
        "The first block that Central Starknet hasn't seen yet",
        CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter().to_string(),
||||||| 912efc99a
        "The first block that Central Starknet hasn't seen yet",
        vec![CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter().to_string()],
=======
        "The first block number that doesn't exist yet",
        CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter().to_string(),
>>>>>>> origin/main-v0.14.1
        PanelType::Stat,
    )
}
fn get_panel_state_sync_body_marker() -> Panel {
    Panel::new(
        "State Sync Body Marker",
        "The first block number for which the state sync component does not have a body",
<<<<<<< HEAD
        STATE_SYNC_BODY_MARKER.get_name_with_filter().to_string(),
||||||| 912efc99a
        vec![STATE_SYNC_BODY_MARKER.get_name_with_filter().to_string()],
=======
        STATE_SYNC_BODY_MARKER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}
fn get_panel_state_sync_class_manager_marker() -> Panel {
    Panel::new(
        "State Sync Class Manager Marker",
        "The first block number for which the state sync component does not have a class",
        STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter().to_string(),
        PanelType::Stat,
    )
}
fn get_panel_state_sync_compiled_class_marker() -> Panel {
    Panel::new(
        "State Sync Compiled Class Marker",
        "The first block number for which the state sync component does not have all of the \
         corresponding compiled classes",
        STATE_SYNC_COMPILED_CLASS_MARKER.get_name_with_filter().to_string(),
>>>>>>> origin/main-v0.14.1
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
<<<<<<< HEAD
        STATE_SYNC_HEADER_LATENCY_SEC.get_name_with_filter().to_string(),
||||||| 912efc99a
        vec![STATE_SYNC_HEADER_LATENCY_SEC.get_name_with_filter().to_string()],
=======
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
             rate).\nThe value is computed from the sync rate of the `class manager marker` \
             (which is the last component to finish downloading among all state sync parts), \
             compared against the `central block marker` (the latest block known to central).",
            DEFAULT_DURATION
        ),
        format!(
            "({target_total} - {sync_state}) / clamp_min(rate({sync_state}[{d}]) - \
             rate({target_total}[{d}]), 1e-6)",
            target_total = CENTRAL_SYNC_CENTRAL_BLOCK_MARKER.get_name_with_filter(),
            sync_state = STATE_SYNC_CLASS_MANAGER_MARKER.get_name_with_filter(),
            d = DEFAULT_DURATION
        ),
>>>>>>> origin/main-v0.14.1
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
}

pub(crate) fn get_state_sync_row() -> Row {
    Row::new(
        "State Sync",
        vec![
            get_panel_central_sync_central_block_marker(),
            get_panel_time_to_complete_sync(),
            get_panel_state_sync_diff_from_central(),
            get_panel_state_sync_new_header_maturity(),
            get_panel_state_sync_body_marker(),
            get_panel_state_sync_class_manager_marker(),
            get_panel_state_sync_compiled_class_marker(),
        ],
    )
}
