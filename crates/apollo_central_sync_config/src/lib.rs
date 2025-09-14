use std::collections::{BTreeMap, HashMap};

use apollo_config::converters::{deserialize_optional_map, serialize_optional_map};
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
