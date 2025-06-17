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

const PANEL_L1_PROVIDER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_PROVIDER_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries);
const PANEL_L1_PROVIDER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(L1_PROVIDER_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries);
const PANEL_L1_PROVIDER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_PROVIDER_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries);
const PANEL_L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries);
const PANEL_L1_PROVIDER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(L1_PROVIDER_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries);
const PANEL_L1_PROVIDER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(L1_PROVIDER_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries);
const PANEL_L1_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS: Panel =
    Panel::from_hist(L1_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries);
const PANEL_L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT: Panel =
    Panel::from_counter(L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT, PanelType::TimeSeries);
const PANEL_L1_MESSAGE_SCRAPER_REORG_DETECTED: Panel =
    Panel::from_counter(L1_MESSAGE_SCRAPER_REORG_DETECTED, PanelType::TimeSeries);

pub(crate) fn get_l1_provider_row() -> Row {
    Row::new(
        "L1 Provider",
        vec![
            PANEL_L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
            PANEL_L1_MESSAGE_SCRAPER_REORG_DETECTED,
        ],
    )
}

pub(crate) fn get_l1_provider_infra_row() -> Row {
    Row::new(
        "L1 Provider Infra",
        vec![
            PANEL_L1_PROVIDER_LOCAL_MSGS_RECEIVED,
            PANEL_L1_PROVIDER_LOCAL_MSGS_PROCESSED,
            PANEL_L1_PROVIDER_LOCAL_QUEUE_DEPTH,
            PANEL_L1_PROVIDER_REMOTE_MSGS_RECEIVED,
            PANEL_L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
            PANEL_L1_PROVIDER_REMOTE_MSGS_PROCESSED,
            PANEL_L1_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
        ],
    )
}
