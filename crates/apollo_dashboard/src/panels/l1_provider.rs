use apollo_l1_provider::metrics::{
    L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
    L1_MESSAGE_SCRAPER_REORG_DETECTED,
    L1_MESSAGE_SCRAPER_SECONDS_SINCE_LAST_SUCCESSFUL_SCRAPE,
    L1_MESSAGE_SCRAPER_SUCCESS_COUNT,
};

use crate::dashboard::{Panel, Row};

fn get_panel_l1_message_scraper_success_count() -> Panel {
    Panel::from(&L1_MESSAGE_SCRAPER_SUCCESS_COUNT)
}
fn get_panel_l1_message_scraper_baselayer_error_count() -> Panel {
    Panel::from(&L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT)
}
fn get_panel_l1_message_scraper_reorg_detected() -> Panel {
    Panel::from(&L1_MESSAGE_SCRAPER_REORG_DETECTED)
}
fn get_panel_l1_message_scraper_seconds_since_last_successful_scrape() -> Panel {
    Panel::from_gauge(
        &L1_MESSAGE_SCRAPER_SECONDS_SINCE_LAST_SUCCESSFUL_SCRAPE,
        PanelType::TimeSeries,
    )
}

// TODO(noamsp): rename to l1_event_row
pub(crate) fn get_l1_provider_row() -> Row {
    Row::new(
        "L1 Provider",
        vec![
            get_panel_l1_message_scraper_success_count(),
            get_panel_l1_message_scraper_baselayer_error_count(),
            get_panel_l1_message_scraper_reorg_detected(),
            get_panel_l1_message_scraper_seconds_since_last_successful_scrape(),
        ],
    )
}
