use apollo_infra::metrics::{
    L1_PROVIDER_LOCAL_MSGS_PROCESSED,
    L1_PROVIDER_LOCAL_MSGS_RECEIVED,
    L1_PROVIDER_LOCAL_QUEUE_DEPTH,
    L1_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
    L1_PROVIDER_REMOTE_MSGS_PROCESSED,
    L1_PROVIDER_REMOTE_MSGS_RECEIVED,
    L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_l1_provider::metrics::{
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_MESSAGE_SCRAPER_REORG_DETECTED,
};

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_l1_provider_local_msgs_received() -> Panel {
    Panel::from_counter(L1_PROVIDER_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_l1_provider_local_msgs_processed() -> Panel {
    Panel::from_counter(L1_PROVIDER_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_l1_provider_remote_msgs_received() -> Panel {
    Panel::from_counter(L1_PROVIDER_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_l1_provider_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_l1_provider_remote_msgs_processed() -> Panel {
    Panel::from_counter(L1_PROVIDER_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_l1_provider_local_queue_depth() -> Panel {
    Panel::from_gauge(L1_PROVIDER_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_panel_l1_provider_remote_client_send_attempts() -> Panel {
    Panel::from_hist(L1_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}
fn get_panel_l1_message_scraper_baselayer_error_count() -> Panel {
    Panel::from_counter(L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT, PanelType::TimeSeries)
}
fn get_panel_l1_message_scraper_reorg_detected() -> Panel {
    Panel::from_counter(L1_MESSAGE_SCRAPER_REORG_DETECTED, PanelType::TimeSeries)
}

pub(crate) fn get_l1_provider_row() -> Row {
    Row::new(
        "L1 Provider",
        vec![
            get_panel_l1_message_scraper_baselayer_error_count(),
            get_panel_l1_message_scraper_reorg_detected(),
        ],
    )
}

pub(crate) fn get_l1_provider_infra_row() -> Row {
    Row::new(
        "L1 Provider Infra",
        vec![
            get_panel_l1_provider_local_msgs_received(),
            get_panel_l1_provider_local_msgs_processed(),
            get_panel_l1_provider_local_queue_depth(),
            get_panel_l1_provider_remote_msgs_received(),
            get_panel_l1_provider_remote_valid_msgs_received(),
            get_panel_l1_provider_remote_msgs_processed(),
            get_panel_l1_provider_remote_client_send_attempts(),
        ],
    )
}
