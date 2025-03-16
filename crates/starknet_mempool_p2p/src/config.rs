use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::NetworkConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

const MEMPOOL_TCP_PORT: u16 = 11111;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct MempoolP2pConfig {
    #[validate]
    pub network_config: NetworkConfig,
    pub network_buffer_size: usize,
    pub max_transaction_batch_size: usize,
    pub transaction_batch_rate_millis: usize,
}

impl Default for MempoolP2pConfig {
    fn default() -> Self {
        Self {
            network_config: NetworkConfig { port: MEMPOOL_TCP_PORT, ..Default::default() },
            network_buffer_size: 10000,
            // TODO(Eitan): Change to appropriate values.
            max_transaction_batch_size: 1,
            transaction_batch_rate_millis: 1000,
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
                    "transaction_batch_size",
                    &self.max_transaction_batch_size,
                    "Maximum number of transactions in each batch.",
                    ParamPrivacyInput::Public,
                ),
                ser_param(
                    "transaction_batch_rate_millis",
                    &self.transaction_batch_rate_millis,
                    "Maximum time until a transaction batch is closed and propagated in \
                     milliseconds.",
                    ParamPrivacyInput::Public,
                ),
            ]),
            append_sub_config_name(self.network_config.dump(), "network_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
