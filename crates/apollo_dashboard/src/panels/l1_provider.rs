use apollo_l1_provider::metrics::{
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_MESSAGE_SCRAPER_REORG_DETECTED,
    L1_MESSAGE_SCRAPER_SUCCESS_COUNT,
};

use crate::dashboard::{get_time_since_last_increase_expr, Panel, PanelType, Row};

fn get_panel_l1_message_scraper_success_count() -> Panel {
    Panel::new(
        "L1 Message Scraper Success Count",
        "The increase in the number of times the L1 message scraper successfully scraped messages \
         (10m window)",
        vec![format!("increase({}[10m])", L1_MESSAGE_SCRAPER_SUCCESS_COUNT.get_name_with_filter())],
        PanelType::TimeSeries,
    )
}
fn get_panel_l1_message_scraper_baselayer_error_count() -> Panel {
    Panel::new(
        "L1 Message Scraper Base Layer Error Count",
        "The increase in the number of times the L1 message scraper encountered an error while \
         scraping the base layer (10m window)",
        vec![format!(
            "increase({}[10m])",
            L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_l1_message_scraper_reorg_detected() -> Panel {
    Panel::new(
        "L1 Message Scraper Reorg Detected",
        "The increase in the number of times the L1 message scraper detected a reorg (12h window)",
        vec![format!(
            "increase({}[12h])",
            L1_MESSAGE_SCRAPER_REORG_DETECTED.get_name_with_filter()
        )],
        PanelType::TimeSeries,
    )
}
fn get_panel_l1_message_scraper_seconds_since_last_successful_scrape() -> Panel {
    Panel::new(
        "Seconds since last successful l1 event scrape",
        "The number of seconds since the last successful scrape of the L1 message scraper \
         (assuming there was a scrape in the last 12 hours)",
        vec![get_time_since_last_increase_expr(
            &L1_MESSAGE_SCRAPER_SUCCESS_COUNT.get_name_with_filter(),
        )],
        PanelType::TimeSeries,
    )
}

// TODO(noamsp): rename to l1_event_row
pub(crate) fn get_l1_provider_row() -> Row {
    Row::new(
        "L1 Provider",
        vec![
            get_panel_l1_message_scraper_seconds_since_last_successful_scrape(),
            get_panel_l1_message_scraper_success_count(),
            get_panel_l1_message_scraper_baselayer_error_count(),
            get_panel_l1_message_scraper_reorg_detected(),
        ],
    )
}
