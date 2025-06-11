use apollo_infra::metrics::{
    L1_PROVIDER_LOCAL_MSGS_PROCESSED,
    L1_PROVIDER_LOCAL_MSGS_RECEIVED,
    L1_PROVIDER_LOCAL_QUEUE_DEPTH,
    L1_PROVIDER_REMOTE_MSGS_PROCESSED,
    L1_PROVIDER_REMOTE_MSGS_RECEIVED,
    L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_l1_provider::metrics::{
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_MESSAGE_SCRAPER_REORG_DETECTED,
};

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_L1_PROVIDER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_PROVIDER_LOCAL_MSGS_RECEIVED, PanelType::Graph);
pub(crate) const PANEL_L1_PROVIDER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(L1_PROVIDER_LOCAL_MSGS_PROCESSED, PanelType::Graph);
pub(crate) const PANEL_L1_PROVIDER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_PROVIDER_REMOTE_MSGS_RECEIVED, PanelType::Graph);
pub(crate) const PANEL_L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, PanelType::Graph);
pub(crate) const PANEL_L1_PROVIDER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(L1_PROVIDER_REMOTE_MSGS_PROCESSED, PanelType::Graph);
pub(crate) const PANEL_L1_PROVIDER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(L1_PROVIDER_LOCAL_QUEUE_DEPTH, PanelType::Graph);
pub(crate) const PANEL_L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT: Panel =
    Panel::from_counter(L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT, PanelType::Graph);
pub(crate) const PANEL_L1_MESSAGE_SCRAPER_REORG_DETECTED: Panel =
    Panel::from_counter(L1_MESSAGE_SCRAPER_REORG_DETECTED, PanelType::Graph);
