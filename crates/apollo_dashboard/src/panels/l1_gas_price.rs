use apollo_l1_gas_price::metrics::{
    ETH_TO_STRK_ERROR_COUNT,
    ETH_TO_STRK_RATE,
    ETH_TO_STRK_SUCCESS_COUNT,
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK,
    L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT,
};
use apollo_l1_gas_price_types::DEFAULT_ETH_TO_FRI_RATE;

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_eth_to_strk_error_count() -> Panel {
    Panel::new(
        "ETH→STRK Rate Query Error Count",
        "The number of times the ETH→STRK rate query failed (10m window)",
        vec![format!("increase({}[10m])", ETH_TO_STRK_ERROR_COUNT.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_eth_to_strk_success_count() -> Panel {
    Panel::new(
        "ETH→STRK Rate Query Success (binary)",
        "Indicates whether the ETH→STRK rate query succeeded (1m window) \nExpected to be 1 every \
         15 minutes.",
        vec![format!("changes({}[1m])", ETH_TO_STRK_SUCCESS_COUNT.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_eth_to_strk_rate() -> Panel {
    Panel::new(
        ETH_TO_STRK_RATE.get_name(),
        format!("ETH→STRK rate (divided by DEFAULT_ETH_TO_FRI_RATE={DEFAULT_ETH_TO_FRI_RATE})"),
        vec![format!("{} / {}", ETH_TO_STRK_RATE.get_name_with_filter(), DEFAULT_ETH_TO_FRI_RATE)],
        PanelType::TimeSeries,
    )
}

fn get_panel_l1_gas_price_provider_insufficient_history() -> Panel {
    Panel::new(
        "L1 Gas Price Provider Insufficient History",
        "The number of times the L1 gas price provider calculated an average with too few blocks \
         (10m window)",
        vec![format!(
            "increase({}[10m])",
            L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_l1_gas_price_scraper_success_count() -> Panel {
    Panel::new(
        "L1 Gas Price Scraper Success Count",
        "The number of times the L1 gas price scraper successfully scraped and updated gas prices \
         (10m window)",
        vec![format!(
            "increase({}[10m])",
            L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_l1_gas_price_scraper_baselayer_error_count() -> Panel {
    Panel::new(
        "L1 Gas Price Scraper Base Layer Error Count",
        "The number of times the L1 gas price scraper encountered an error while scraping the \
         base layer (10m window)",
        vec![format!(
            "increase({}[10m])",
            L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_l1_gas_price_scraper_reorg_detected() -> Panel {
    Panel::new(
        "L1 Gas Price Scraper Reorg Detected",
        "The number of times the L1 gas price scraper detected a reorganization in the base layer \
         (10m window)",
        vec![format!(
            "increase({}[10m])",
            L1_GAS_PRICE_SCRAPER_REORG_DETECTED.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}

fn get_panel_l1_gas_price_scraper_latest_scraped_block() -> Panel {
    Panel::new(
        "L1 Gas Price Scraper Latest Scraped Block",
        "The latest block number that the L1 gas price scraper has scraped",
        vec![format!("{}", L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK.get_name_with_filter())],
        PanelType::Stat,
    )
}

fn get_panel_l1_gas_price_latest_mean_value() -> Panel {
    Panel::from_gauge(
        &apollo_l1_gas_price::metrics::L1_GAS_PRICE_LATEST_MEAN_VALUE,
        PanelType::TimeSeries,
    )
}

fn get_panel_l1_data_gas_price_latest_mean_value() -> Panel {
    Panel::from_gauge(
        &apollo_l1_gas_price::metrics::L1_DATA_GAS_PRICE_LATEST_MEAN_VALUE,
        PanelType::TimeSeries,
    )
}

pub(crate) fn get_l1_gas_price_row() -> Row {
    Row::new(
        "ETH→STRK Rate & L1 Gas Price",
        vec![
            get_panel_eth_to_strk_success_count(),
            get_panel_eth_to_strk_error_count(),
            get_panel_eth_to_strk_rate(),
            get_panel_l1_gas_price_provider_insufficient_history(),
            get_panel_l1_gas_price_scraper_success_count(),
            get_panel_l1_gas_price_scraper_baselayer_error_count(),
            get_panel_l1_gas_price_scraper_reorg_detected(),
            get_panel_l1_gas_price_scraper_latest_scraped_block(),
            get_panel_l1_gas_price_latest_mean_value(),
            get_panel_l1_data_gas_price_latest_mean_value(),
        ],
    )
}
