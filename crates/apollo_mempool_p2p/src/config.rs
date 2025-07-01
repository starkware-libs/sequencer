use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::deserialize_milliseconds_to_duration;
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_network::NetworkConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

const MEMPOOL_UDP_PORT: u16 = 11111;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct MempoolP2pConfig {
    #[validate]
    pub network_config: NetworkConfig,
    pub network_buffer_size: usize,
    pub max_transaction_batch_size: usize,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub transaction_batch_rate_millis: Duration,
}

impl Default for MempoolP2pConfig {
    fn default() -> Self {
        Self {
            network_config: NetworkConfig { port: MEMPOOL_UDP_PORT, ..Default::default() },
            network_buffer_size: 10000,
            // TODO(Eitan): Change to appropriate values.
            max_transaction_batch_size: 1,
            transaction_batch_rate_millis: Duration::from_secs(1),
        }
    }
}

impl SerializeConfig for MempoolP2pConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            BTreeMap::from_iter([
                ser_param(
                    "network_buffer_size",
                    &self.network_buffer_size,
                    "Network buffer size.",
                    ParamPrivacyInput::Public,
                ),
                ser_param(
                    "max_transaction_batch_size",
                    &self.max_transaction_batch_size,
                    "Maximum number of transactions in each batch.",
                    ParamPrivacyInput::Public,
                ),
                ser_param(
                    "transaction_batch_rate_millis",
                    &self.transaction_batch_rate_millis.as_millis(),
                    "Maximum time until a transaction batch is closed and propagated in \
                     milliseconds.",
                    ParamPrivacyInput::Public,
                ),
            ]),
            prepend_sub_config_name(self.network_config.dump(), "network_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
