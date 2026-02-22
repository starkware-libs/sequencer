pub mod catchupper;

pub mod communication;
pub mod l1_events_provider;
pub mod l1_scraper;
pub mod metrics;

pub(crate) mod transaction_manager;
pub(crate) mod transaction_record;

#[cfg(any(test, feature = "testing"))]
pub mod test_utils;

pub use apollo_l1_events_config::config::L1EventsProviderConfig;
use papyrus_base_layer::constants::{
    EventIdentifier,
    CONSUMED_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELED_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
};

pub const fn event_identifiers_to_track() -> &'static [EventIdentifier] {
    &[
        // LogMessageToL2(address,uint256,uint256,uint256[],uint256,uint256)
        LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
        // MessageToL2CancellationStarted(address,uint256,uint256)
        MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
        // MessageToL2Canceled(address,uint256,uint256,uint256[],uint256,uint256)
        MESSAGE_TO_L2_CANCELED_EVENT_IDENTIFIER,
        // ConsumedMessageToL2(address,uint256,uint256,uint256[],uint256,uint256)
        CONSUMED_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    ]
}
