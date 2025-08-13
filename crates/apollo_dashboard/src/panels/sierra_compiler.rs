use apollo_compile_to_casm::metrics::COMPILATION_DURATION;
use apollo_infra::metrics::{
    SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
    SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
    SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
    SIERRA_COMPILER_PROCESSING_TIMES,
    SIERRA_COMPILER_QUEUEING_TIMES,
    SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS,
    SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
    SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
    SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
};

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_local_msgs_received() -> Panel {
    Panel::from_counter(SIERRA_COMPILER_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_local_msgs_processed() -> Panel {
    Panel::from_counter(SIERRA_COMPILER_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_remote_msgs_received() -> Panel {
    Panel::from_counter(SIERRA_COMPILER_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_remote_msgs_processed() -> Panel {
    Panel::from_counter(SIERRA_COMPILER_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_local_queue_depth() -> Panel {
    Panel::from_gauge(SIERRA_COMPILER_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_panel_remote_client_send_attempts() -> Panel {
    Panel::from_hist(SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}
fn get_panel_processing_times() -> Panel {
    Panel::from_hist(SIERRA_COMPILER_PROCESSING_TIMES, PanelType::TimeSeries)
}
fn get_panel_queueing_times() -> Panel {
    Panel::from_hist(SIERRA_COMPILER_QUEUEING_TIMES, PanelType::TimeSeries)
}

fn get_panel_compilation_duration() -> Panel {
    Panel::from_hist(COMPILATION_DURATION, PanelType::TimeSeries)
}

pub(crate) fn get_sierra_compiler_infra_row() -> Row {
    Row::new(
        "SierraCompilerInfra",
        vec![
            get_panel_local_msgs_received(),
            get_panel_local_msgs_processed(),
            get_panel_local_queue_depth(),
            get_panel_remote_msgs_received(),
            get_panel_remote_valid_msgs_received(),
            get_panel_remote_msgs_processed(),
            get_panel_remote_client_send_attempts(),
            get_panel_processing_times(),
            get_panel_queueing_times(),
        ],
    )
}

pub(crate) fn get_compile_to_casm_row() -> Row {
    Row::new("Compile sierra to casm", vec![get_panel_compilation_duration()])
}
