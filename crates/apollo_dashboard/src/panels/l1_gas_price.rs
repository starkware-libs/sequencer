use apollo_infra::metrics::{
    L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED,
    L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED,
    L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH,
    L1_GAS_PRICE_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
    L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED,
    L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED,
    L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_l1_gas_price::metrics::{
    ETH_TO_STRK_ERROR_COUNT,
    ETH_TO_STRK_SUCCESS_COUNT,
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
};

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_l1_gas_price_provider_local_msgs_received() -> Panel {
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_l1_gas_price_provider_local_msgs_processed() -> Panel {
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_l1_gas_price_provider_remote_msgs_received() -> Panel {
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_l1_gas_price_provider_remote_valid_msgs_received() -> Panel {
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED, PanelType::TimeSeries)
}
fn get_panel_l1_gas_price_provider_remote_msgs_processed() -> Panel {
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED, PanelType::TimeSeries)
}
fn get_panel_l1_gas_price_provider_local_queue_depth() -> Panel {
    Panel::from_gauge(L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH, PanelType::TimeSeries)
}
fn get_panel_l1_gas_price_provider_remote_client_send_attempts() -> Panel {
    Panel::from_hist(L1_GAS_PRICE_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS, PanelType::TimeSeries)
}

fn get_panel_l1_gas_price_provider_insufficient_history() -> Panel {
    Panel::from_counter(L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY, PanelType::Stat)
}
fn get_panel_l1_gas_price_scraper_baselayer_error_count() -> Panel {
    Panel::from_counter(L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT, PanelType::Stat)
}
fn get_panel_l1_gas_price_scraper_reorg_detected() -> Panel {
    Panel::from_counter(L1_GAS_PRICE_SCRAPER_REORG_DETECTED, PanelType::Stat)
}
fn get_panel_eth_to_strk_error_count() -> Panel {
    Panel::from_counter(ETH_TO_STRK_ERROR_COUNT, PanelType::Stat)
}
fn get_panel_eth_to_strk_success_count() -> Panel {
    Panel::from_counter(ETH_TO_STRK_SUCCESS_COUNT, PanelType::Stat)
}

pub(crate) fn get_l1_gas_price_row() -> Row {
    Row::new(
        "L1 Gas Price",
        vec![
            get_panel_eth_to_strk_error_count(),
            get_panel_eth_to_strk_success_count(),
            get_panel_l1_gas_price_provider_insufficient_history(),
            get_panel_l1_gas_price_scraper_baselayer_error_count(),
            get_panel_l1_gas_price_scraper_reorg_detected(),
        ],
    )
}

pub(crate) fn get_l1_gas_price_infra_row() -> Row {
    Row::new(
        "L1 Gas Price Infra",
        vec![
            get_panel_l1_gas_price_provider_local_msgs_received(),
            get_panel_l1_gas_price_provider_local_msgs_processed(),
            get_panel_l1_gas_price_provider_local_queue_depth(),
            get_panel_l1_gas_price_provider_remote_msgs_received(),
            get_panel_l1_gas_price_provider_remote_valid_msgs_received(),
            get_panel_l1_gas_price_provider_remote_msgs_processed(),
            get_panel_l1_gas_price_provider_remote_client_send_attempts(),
        ],
    )
}
