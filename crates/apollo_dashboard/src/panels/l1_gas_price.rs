use apollo_infra::metrics::{
    L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED,
    L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED,
    L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH,
    L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED,
    L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED,
    L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_l1_gas_price::metrics::{
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
};

use crate::dashboard::{Panel, PanelType};

pub(crate) const PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED, PanelType::Stat);
pub(crate) const PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, PanelType::Stat);
pub(crate) const PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED, PanelType::Stat);
pub(crate) const PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH: Panel =
    Panel::from_gauge(L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH, PanelType::Stat);
pub(crate) const PANEL_L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY: Panel =
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY, PanelType::Stat);
pub(crate) const PANEL_L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT: Panel =
    Panel::from_counter(L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT, PanelType::Stat);
