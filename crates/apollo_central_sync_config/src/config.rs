use std::collections::HashMap;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_optional_sensitive_map,
    deserialize_seconds_to_duration,
    serialize_duration_as_milliseconds,
    serialize_duration_as_seconds,
    serialize_optional_sensitive_map,
};
use apollo_config::secrets::Sensitive;
use apollo_starknet_client::RetryConfig;
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct CentralSourceConfig {
    pub concurrent_requests: usize,
    pub starknet_url: Url,
    #[serde(
        serialize_with = "serialize_optional_sensitive_map",
        deserialize_with = "deserialize_optional_sensitive_map"
    )]
    pub http_headers: Option<Sensitive<HashMap<String, String>>>,
    pub max_state_updates_to_download: usize,
    pub max_state_updates_to_store_in_memory: usize,
    pub max_classes_to_download: usize,
    #[validate(range(min = 1))]
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SyncConfig {
    #[serde(
        serialize_with = "serialize_duration_as_milliseconds",
        deserialize_with = "deserialize_milliseconds_to_duration"
    )]
    pub latest_block_poll_interval_millis: Duration,
    #[serde(
        serialize_with = "serialize_duration_as_seconds",
        deserialize_with = "deserialize_seconds_to_duration"
    )]
    pub base_layer_propagation_sleep_duration: Duration,
    #[serde(
        serialize_with = "serialize_duration_as_seconds",
        deserialize_with = "deserialize_seconds_to_duration"
    )]
    pub recoverable_error_sleep_duration: Duration,
    pub blocks_max_stream_size: u32,
    pub state_updates_max_stream_size: u32,
    pub verify_blocks: bool,
    pub collect_pending_data: bool,
    pub store_sierras_and_casms_block_threshold: u64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            latest_block_poll_interval_millis: Duration::from_millis(500),
            base_layer_propagation_sleep_duration: Duration::from_secs(10),
            recoverable_error_sleep_duration: Duration::from_secs(3),
            blocks_max_stream_size: 1000,
            state_updates_max_stream_size: 1000,
            verify_blocks: true,
            collect_pending_data: false,
            store_sierras_and_casms_block_threshold: 0,
        }
    }
}
