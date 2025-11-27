pub mod bootstrapper;

pub mod communication;
pub mod l1_provider;
pub mod l1_scraper;
pub mod metrics;

pub(crate) mod transaction_manager;
pub(crate) mod transaction_record;

#[cfg(any(test, feature = "testing"))]
pub mod test_utils;

pub use apollo_l1_provider_config::config::L1ProviderConfig;
use papyrus_base_layer::constants::{
    EventIdentifier,
    CONSUMED_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELED_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
};

use crate::transaction_manager::TransactionManagerConfig;

// TODO(Nadin): Move to the l1 provider config crate.
impl From<L1ProviderConfig> for TransactionManagerConfig {
    fn from(config: L1ProviderConfig) -> Self {
        TransactionManagerConfig {
            new_l1_handler_tx_cooldown_secs: config.new_l1_handler_cooldown_seconds,
            l1_handler_cancellation_timelock_seconds: config
                .l1_handler_cancellation_timelock_seconds,
            l1_handler_consumption_timelock_seconds: config.l1_handler_consumption_timelock_seconds,
        }
    }
}

pub const fn event_identifiers_to_track() -> &'static [EventIdentifier] {
    &[
        LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
        MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
        MESSAGE_TO_L2_CANCELED_EVENT_IDENTIFIER,
        CONSUMED_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    ]
}
