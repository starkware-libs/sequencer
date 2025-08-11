use apollo_infra::metrics::{
    L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED,
    L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED,
    L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH,
    L1_GAS_PRICE_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
    L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED,
    L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED,
    L1_GAS_PRICE_PROVIDER_REMOTE_NUMBER_OF_CONNECTIONS,
    L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_l1_gas_price::metrics::{
    ETH_TO_STRK_ERROR_COUNT,
    ETH_TO_STRK_RATE,
    ETH_TO_STRK_SUCCESS_COUNT,
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT,
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
fn get_panel_l1_gas_price_provider_remote_number_of_connections() -> Panel {
    Panel::from_gauge(L1_GAS_PRICE_PROVIDER_REMOTE_NUMBER_OF_CONNECTIONS, PanelType::TimeSeries)
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
fn get_panel_l1_gas_price_scraper_success_count() -> Panel {
    Panel::from_counter(L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT, PanelType::Stat)
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

fn get_panel_l1_gas_price_scraper_latest_scraped_block() -> Panel {
    Panel::from_gauge(
        apollo_l1_gas_price::metrics::L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK,
        PanelType::TimeSeries,
    )
}

fn get_panel_eth_to_strk_rate() -> Panel {
    Panel::from_gauge(ETH_TO_STRK_RATE, PanelType::TimeSeries)
}

fn get_panel_l1_gas_price_latest_mean_value() -> Panel {
    Panel::from_gauge(
        apollo_l1_gas_price::metrics::L1_GAS_PRICE_LATEST_MEAN_VALUE,
        PanelType::TimeSeries,
    )
}

fn get_panel_l1_data_gas_price_latest_mean_value() -> Panel {
    Panel::from_gauge(
        apollo_l1_gas_price::metrics::L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE,
        PanelType::TimeSeries,
    )
}

pub(crate) fn get_l1_gas_price_row() -> Row {
    Row::new(
        "L1 Gas Price",
        vec![
            get_panel_eth_to_strk_error_count(),
            get_panel_eth_to_strk_success_count(),
            get_panel_eth_to_strk_rate(),
            get_panel_l1_gas_price_provider_insufficient_history(),
            get_panel_l1_gas_price_scraper_success_count(),
            get_panel_l1_gas_price_scraper_baselayer_error_count(),
            get_panel_l1_gas_price_scraper_reorg_detected(),
            get_panel_l1_gas_price_scraper_latest_scraped_block(),
            get_panel_eth_to_strk_rate(),
            get_panel_l1_gas_price_latest_mean_value(),
            get_panel_l1_data_gas_price_latest_mean_value(),
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
            get_panel_l1_gas_price_provider_remote_number_of_connections(),
            get_panel_l1_gas_price_provider_remote_client_send_attempts(),
        ],
    )
}
