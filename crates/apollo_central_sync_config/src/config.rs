use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use apollo_config::converters::{
    deserialize_optional_map,
    deserialize_seconds_to_duration,
    serialize_optional_map,
};
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_starknet_client::RetryConfig;
use itertools::chain;
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct CentralSourceConfig {
    pub concurrent_requests: usize,
    pub starknet_url: Url,
    #[serde(deserialize_with = "deserialize_optional_map")]
    pub http_headers: Option<HashMap<String, String>>,
    pub max_state_updates_to_download: usize,
    pub max_state_updates_to_store_in_memory: usize,
    pub max_classes_to_download: usize,
    // TODO(dan): validate that class_cache_size is a positive integer.
    pub class_cache_size: usize,
    pub retry_config: RetryConfig,
}

impl Default for CentralSourceConfig {
    fn default() -> Self {
        CentralSourceConfig {
            concurrent_requests: 10,
            starknet_url: Url::parse("https://alpha-mainnet.starknet.io/")
                .expect("Unable to parse default URL, this should never happen."),
            http_headers: None,
            max_state_updates_to_download: 20,
            max_state_updates_to_store_in_memory: 20,
            max_classes_to_download: 20,
            class_cache_size: 100,
            retry_config: RetryConfig {
                retry_base_millis: 30,
                retry_max_delay_millis: 30000,
                max_retries: 10,
            },
        }
    }
}

impl SerializeConfig for CentralSourceConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let self_params_dump = BTreeMap::from_iter([
            ser_param(
                "concurrent_requests",
                &self.concurrent_requests,
                "Maximum number of concurrent requests to Starknet feeder-gateway for getting a \
                 type of data (for example, blocks).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "starknet_url",
                &self.starknet_url,
                "Starknet feeder-gateway URL. It should match chain_id.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "http_headers",
                &serialize_optional_map(&self.http_headers),
                "'k1:v1 k2:v2 ...' headers for SN-client.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "max_state_updates_to_download",
                &self.max_state_updates_to_download,
                "Maximum number of state updates to download at a given time.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_state_updates_to_store_in_memory",
                &self.max_state_updates_to_store_in_memory,
                "Maximum number of state updates to store in memory at a given time.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_classes_to_download",
                &self.max_classes_to_download,
                "Maximum number of classes to download at a given time.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "class_cache_size",
                &self.class_cache_size,
                "Size of class cache, must be a positive integer.",
                ParamPrivacyInput::Public,
            ),
        ]);
        chain!(self_params_dump, prepend_sub_config_name(self.retry_config.dump(), "retry_config"))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SyncConfig {
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub block_propagation_sleep_duration: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub base_layer_propagation_sleep_duration: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub recoverable_error_sleep_duration: Duration,
    pub blocks_max_stream_size: u32,
    pub state_updates_max_stream_size: u32,
    pub verify_blocks: bool,
    pub collect_pending_data: bool,
}

impl SerializeConfig for SyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "block_propagation_sleep_duration",
                &self.block_propagation_sleep_duration.as_secs(),
                "Time in seconds before checking for a new block after the node is synchronized.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "base_layer_propagation_sleep_duration",
                &self.base_layer_propagation_sleep_duration.as_secs(),
                "Time in seconds to poll the base layer to get the latest proved block.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "recoverable_error_sleep_duration",
                &self.recoverable_error_sleep_duration.as_secs(),
                "Waiting time in seconds before restarting synchronization after a recoverable \
                 error.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "blocks_max_stream_size",
                &self.blocks_max_stream_size,
                "Max amount of blocks to download in a stream.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "state_updates_max_stream_size",
                &self.state_updates_max_stream_size,
                "Max amount of state updates to download in a stream.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "verify_blocks",
                &self.verify_blocks,
                "Whether to verify incoming blocks.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "collect_pending_data",
                &self.collect_pending_data,
                "Whether to collect data on pending blocks.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            block_propagation_sleep_duration: Duration::from_secs(2),
            base_layer_propagation_sleep_duration: Duration::from_secs(10),
            recoverable_error_sleep_duration: Duration::from_secs(3),
            blocks_max_stream_size: 1000,
            state_updates_max_stream_size: 1000,
            verify_blocks: true,
            collect_pending_data: false,
        }
    }
}
