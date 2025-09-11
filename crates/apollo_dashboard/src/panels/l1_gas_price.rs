use apollo_l1_gas_price::metrics::{
    ETH_TO_STRK_ERROR_COUNT,
    ETH_TO_STRK_RATE,
    ETH_TO_STRK_SUCCESS_COUNT,
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT,
};

use crate::dashboard::{Panel, Row};

fn get_panel_insufficient_history() -> Panel {
    Panel::from(&L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY)
}
fn get_panel_l1_gas_price_scraper_success_count() -> Panel {
    Panel::from(&L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT)
}
fn get_panel_l1_gas_price_scraper_baselayer_error_count() -> Panel {
    Panel::from(&L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT)
}
fn get_panel_l1_gas_price_scraper_reorg_detected() -> Panel {
    Panel::from(&L1_GAS_PRICE_SCRAPER_REORG_DETECTED)
}
fn get_panel_eth_to_strk_error_count() -> Panel {
    Panel::from(&ETH_TO_STRK_ERROR_COUNT)
}
fn get_panel_eth_to_strk_success_count() -> Panel {
    Panel::from(&ETH_TO_STRK_SUCCESS_COUNT)
}

fn get_panel_l1_gas_price_scraper_latest_scraped_block() -> Panel {
    Panel::from(&apollo_l1_gas_price::metrics::L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK)
}

fn get_panel_eth_to_strk_rate() -> Panel {
    Panel::from(&ETH_TO_STRK_RATE)
}

fn get_panel_l1_gas_price_latest_mean_value() -> Panel {
    Panel::from(&apollo_l1_gas_price::metrics::L1_GAS_PRICE_LATEST_MEAN_VALUE)
}

fn get_panel_l1_data_gas_price_latest_mean_value() -> Panel {
    Panel::from(&apollo_l1_gas_price::metrics::L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE)
}

pub(crate) fn get_l1_gas_price_row() -> Row {
    Row::new(
        "L1 Gas Price",
        vec![
            get_panel_eth_to_strk_error_count(),
            get_panel_eth_to_strk_success_count(),
            get_panel_eth_to_strk_rate(),
            get_panel_insufficient_history(),
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
