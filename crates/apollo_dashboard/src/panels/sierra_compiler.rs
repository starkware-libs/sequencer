use apollo_compile_to_casm::metrics::{
    COMPILATION_DURATION,
    SIERRA_COMPILER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    SIERRA_COMPILER_LABELED_PROCESSING_TIMES_SECS,
    SIERRA_COMPILER_LABELED_QUEUEING_TIMES_SECS,
    SIERRA_COMPILER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_infra::metrics::{
    SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
    SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
    SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
    SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS,
    SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
    SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
    SIERRA_COMPILER_REMOTE_NUMBER_OF_CONNECTIONS,
    SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
};

use crate::dashboard::{create_request_type_labeled_hist_panels, Panel, PanelType, Row};

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
fn get_panel_remote_number_of_connections() -> Panel {
    Panel::from_gauge(SIERRA_COMPILER_REMOTE_NUMBER_OF_CONNECTIONS, PanelType::TimeSeries)
}

fn get_processing_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        SIERRA_COMPILER_LABELED_PROCESSING_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_queueing_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        SIERRA_COMPILER_LABELED_QUEUEING_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_panel_local_client_response_times() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        SIERRA_COMPILER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_panel_remote_client_response_times() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        SIERRA_COMPILER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
        PanelType::TimeSeries,
    )
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
            get_panel_remote_number_of_connections(),
            get_panel_remote_client_send_attempts(),
        ]
        .into_iter()
        .chain(get_processing_times_panels())
        .chain(get_queueing_times_panels())
        .chain(get_panel_local_client_response_times())
        .chain(get_panel_remote_client_response_times())
        .collect::<Vec<_>>(),
    )
}

pub(crate) fn get_compile_to_casm_row() -> Row {
    Row::new("Compile sierra to casm", vec![get_panel_compilation_duration()])
}
