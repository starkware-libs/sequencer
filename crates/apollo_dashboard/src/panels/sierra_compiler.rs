use apollo_compile_to_casm::metrics::COMPILATION_DURATION;
use apollo_infra::metrics::{
    SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
    SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
    SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
    SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS,
    SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
    SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
    SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
};
use const_format::formatcp;

use crate::dashboard::{Panel, PanelType, Row};

const PANEL_SIERRA_COMPILER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(SIERRA_COMPILER_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries);
const PANEL_SIERRA_COMPILER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(SIERRA_COMPILER_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries);
const PANEL_SIERRA_COMPILER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(SIERRA_COMPILER_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries);
const PANEL_SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries);
const PANEL_SIERRA_COMPILER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(SIERRA_COMPILER_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries);
const PANEL_SIERRA_COMPILER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(SIERRA_COMPILER_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries);
const PANEL_SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS: Panel =
    Panel::from_hist(SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries);

const PANEL_COMPILATION_DURATION: Panel = Panel::new(
    COMPILATION_DURATION.get_name_with_filter(),
    COMPILATION_DURATION.get_description(),
    formatcp!("avg_over_time({}[2m])", COMPILATION_DURATION.get_name_with_filter()),
    PanelType::TimeSeries,
);

pub(crate) fn get_sierra_compiler_infra_row() -> Row {
    Row::new(
        "SierraCompilerInfra",
        vec![
            PANEL_SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
            PANEL_SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
            PANEL_SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
            PANEL_SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
            PANEL_SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
            PANEL_SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
            PANEL_SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS,
        ],
    )
}

pub(crate) fn get_compile_to_casm_row() -> Row {
    Row::new("Compile sierra to casm", vec![PANEL_COMPILATION_DURATION])
}
