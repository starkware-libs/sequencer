use apollo_class_manager::metrics::{
    CLASS_MANAGER_LABELED_PROCESSING_TIMES_SECS,
    CLASS_MANAGER_LABELED_QUEUEING_TIMES_SECS,
};
use apollo_infra::metrics::{
    CLASS_MANAGER_LOCAL_MSGS_PROCESSED,
    CLASS_MANAGER_LOCAL_MSGS_RECEIVED,
    CLASS_MANAGER_LOCAL_QUEUE_DEPTH,
    CLASS_MANAGER_REMOTE_CLIENT_SEND_ATTEMPTS,
    CLASS_MANAGER_REMOTE_MSGS_PROCESSED,
    CLASS_MANAGER_REMOTE_MSGS_RECEIVED,
    CLASS_MANAGER_REMOTE_NUMBER_OF_CONNECTIONS,
    CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED,
};

use crate::dashboard::{create_request_type_labeled_hist_panels, Panel, PanelType, Row};

fn get_panel_local_msgs_received() -> Panel {
    Panel::from_counter(CLASS_MANAGER_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_local_msgs_processed() -> Panel {
    Panel::from_counter(CLASS_MANAGER_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_remote_msgs_received() -> Panel {
    Panel::from_counter(CLASS_MANAGER_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_remote_msgs_processed() -> Panel {
    Panel::from_counter(CLASS_MANAGER_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_remote_number_of_connections() -> Panel {
    Panel::from_gauge(CLASS_MANAGER_REMOTE_NUMBER_OF_CONNECTIONS, PanelType::TimeSeries)
}
fn get_panel_local_queue_depth() -> Panel {
    Panel::from_gauge(CLASS_MANAGER_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_panel_remote_client_send_attempts() -> Panel {
    Panel::from_hist(CLASS_MANAGER_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}
fn get_processing_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        CLASS_MANAGER_LABELED_PROCESSING_TIMES_SECS,
        PanelType::TimeSeries,
    )
}
fn get_queueing_times_panels() -> Vec<Panel> {
    create_request_type_labeled_hist_panels(
        CLASS_MANAGER_LABELED_QUEUEING_TIMES_SECS,
        PanelType::TimeSeries,
    )
}

pub(crate) fn get_class_manager_infra_row() -> Row {
    Row::new(
        "Class Manager Infra",
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
        .collect::<Vec<_>>(),
    )
}
