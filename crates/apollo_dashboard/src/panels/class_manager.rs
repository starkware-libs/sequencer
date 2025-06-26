use apollo_infra::metrics::{
    CLASS_MANAGER_LOCAL_MSGS_PROCESSED,
    CLASS_MANAGER_LOCAL_MSGS_RECEIVED,
    CLASS_MANAGER_LOCAL_QUEUE_DEPTH,
    CLASS_MANAGER_REMOTE_CLIENT_SEND_ATTEMPTS,
    CLASS_MANAGER_REMOTE_MSGS_PROCESSED,
    CLASS_MANAGER_REMOTE_MSGS_RECEIVED,
    CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED,
};

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_class_manager_local_msgs_received() -> Panel {
    Panel::from_counter(CLASS_MANAGER_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_class_manager_local_msgs_processed() -> Panel {
    Panel::from_counter(CLASS_MANAGER_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_class_manager_remote_msgs_received() -> Panel {
    Panel::from_counter(CLASS_MANAGER_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_class_manager_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_class_manager_remote_msgs_processed() -> Panel {
    Panel::from_counter(CLASS_MANAGER_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_class_manager_local_queue_depth() -> Panel {
    Panel::from_gauge(CLASS_MANAGER_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_panel_class_manager_remote_client_send_attempts() -> Panel {
    Panel::from_hist(CLASS_MANAGER_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}

pub(crate) fn get_class_manager_infra_row() -> Row {
    Row::new(
        "Class Manager Infra",
        vec![
            get_panel_class_manager_local_msgs_received(),
            get_panel_class_manager_local_msgs_processed(),
            get_panel_class_manager_local_queue_depth(),
            get_panel_class_manager_remote_msgs_received(),
            get_panel_class_manager_remote_valid_msgs_received(),
            get_panel_class_manager_remote_msgs_processed(),
            get_panel_class_manager_remote_client_send_attempts(),
        ],
    )
}
