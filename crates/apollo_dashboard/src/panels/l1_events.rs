use apollo_l1_events::metrics::{
    L1_MESSAGE_PROVIDER_NUM_PENDING_TXS,
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_MESSAGE_SCRAPER_L1_HANDLER_TX_COUNT,
    L1_MESSAGE_SCRAPER_LAST_SUCCESS_TIMESTAMP_SECONDS,
    L1_MESSAGE_SCRAPER_LATEST_SCRAPED_BLOCK,
    L1_MESSAGE_SCRAPER_REORG_DETECTED,
    L1_MESSAGE_SCRAPER_SUCCESS_COUNT,
};

use crate::dashboard::Row;
use crate::panel::{traffic_light_thresholds, Panel, PanelType, Unit};
use crate::query_builder::{increase, seconds_since_last_timestamp, DEFAULT_DURATION};

fn get_panel_l1_message_scraper_success_count() -> Panel {
    Panel::new(
        "L1 Message Scraper Success Count",
        format!(
            "The increase in the number of times the L1 message scraper successfully scraped \
             messages ({DEFAULT_DURATION} window)",
        ),
        increase(&L1_MESSAGE_SCRAPER_SUCCESS_COUNT, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
}
fn get_panel_l1_message_scraper_baselayer_error_count() -> Panel {
    Panel::new(
        "L1 Message Scraper Base Layer Error Count",
        format!(
            "The increase in the number of times the L1 message scraper encountered an error \
             while scraping the base layer ({DEFAULT_DURATION} window)",
        ),
        increase(&L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT, DEFAULT_DURATION),
        PanelType::TimeSeries,
    )
    .with_log_query("BaseLayerError during scraping")
}
fn get_panel_l1_message_scraper_reorg_detected() -> Panel {
    Panel::new(
        "L1 Message Scraper Reorg Detected",
        "The increase in the number of times the L1 message scraper detected a reorg (12h window)",
        increase(&L1_MESSAGE_SCRAPER_REORG_DETECTED, "12h"),
        PanelType::TimeSeries,
    )
    .with_log_query("L1 reorg detected")
}
fn get_panel_l1_message_scraper_latest_scraped_block() -> Panel {
    Panel::from_gauge(&L1_MESSAGE_SCRAPER_LATEST_SCRAPED_BLOCK, PanelType::TimeSeries)
}
fn get_panel_l1_events_num_pending_txs() -> Panel {
    Panel::from_gauge(&L1_MESSAGE_PROVIDER_NUM_PENDING_TXS, PanelType::TimeSeries)
}
fn get_panel_l1_message_scraper_l1_handler_tx_rate() -> Panel {
    Panel::new(
        "L1 Handler Tx Scrape Rate (per minute)",
        "Number of unique L1 handler transactions scraped from L1 over the last 1m window",
        increase(&L1_MESSAGE_SCRAPER_L1_HANDLER_TX_COUNT, "1m"),
        PanelType::TimeSeries,
    )
}

fn get_panel_l1_message_scraper_seconds_since_last_successful_scrape() -> Panel {
    Panel::new(
        "Seconds Since Last Successful L1 Event Scrape",
        "The number of seconds since the last successful scrape of the L1 message scraper \
         (assuming there was a scrape in the last 12 hours)",
        seconds_since_last_timestamp(&L1_MESSAGE_SCRAPER_LAST_SUCCESS_TIMESTAMP_SECONDS),
        PanelType::TimeSeries,
    )
    .with_unit(Unit::Seconds)
    .with_absolute_thresholds(traffic_light_thresholds(30.0, 120.0))
    .with_log_query("BaseLayerError during scraping")
}

pub(crate) fn get_l1_events_row() -> Row {
    Row::new(
        "L1 Events",
        vec![
            get_panel_l1_message_scraper_seconds_since_last_successful_scrape(),
            get_panel_l1_message_scraper_latest_scraped_block(),
            get_panel_l1_events_num_pending_txs(),
            get_panel_l1_message_scraper_l1_handler_tx_rate(),
            get_panel_l1_message_scraper_success_count(),
            get_panel_l1_message_scraper_baselayer_error_count(),
            get_panel_l1_message_scraper_reorg_detected(),
        ],
    )
}
