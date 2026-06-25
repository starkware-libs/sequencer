use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    serialize_duration_as_milliseconds,
};
use apollo_storage::db::DbConfig;
use apollo_storage::storage_reader_server::{
    StorageReaderServerDynamicConfig,
    StorageReaderServerStaticConfig,
};
use apollo_storage::{StorageConfig, StorageScope};
use blockifier::blockifier::config::{
    ContractClassManagerConfig,
    NativeClassesWhitelist,
    WorkerPoolConfig,
};
use blockifier::blockifier_versioned_constants::VersionedConstantsOverrides;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::ChainInfo;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use url::Url;
use validator::{Validate, ValidationError};

pub const DEFAULT_TASKS_CHANNEL_SIZE: usize = 1000;
pub const DEFAULT_RESULTS_CHANNEL_SIZE: usize = 1000;

/// Configuration for the block builder component of the batcher.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BlockBuilderConfig {
    pub chain_info: ChainInfo,
    pub execute_config: WorkerPoolConfig,
    pub bouncer_config: BouncerConfig,
    pub versioned_constants_overrides: Option<VersionedConstantsOverrides>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitmentManagerConfig {
    pub tasks_channel_size: usize,
    pub results_channel_size: usize,
    pub panic_if_task_channel_full: bool,
}

impl Default for CommitmentManagerConfig {
    fn default() -> Self {
        Self {
            tasks_channel_size: DEFAULT_TASKS_CHANNEL_SIZE,
            results_channel_size: DEFAULT_RESULTS_CHANNEL_SIZE,
            panic_if_task_channel_full: false,
        }
    }
}

/// Configuration for the preconfirmed block writer component of the batcher.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct PreconfirmedBlockWriterConfig {
    pub channel_buffer_capacity: usize,
    pub write_block_interval_millis: u64,
}

impl Default for PreconfirmedBlockWriterConfig {
    fn default() -> Self {
        Self { channel_buffer_capacity: 1000, write_block_interval_millis: 50 }
    }
}

/// Configuration for the preconfirmed Cende client component of the batcher.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PreconfirmedCendeConfig {
    pub recorder_url: Url,
}

impl Default for PreconfirmedCendeConfig {
    fn default() -> Self {
        Self {
            recorder_url: "https://recorder_url"
                .parse::<Url>()
                .expect("recorder_url must be a valid Recorder URL"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct FirstBlockWithPartialBlockHash {
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub parent_block_hash: BlockHash,
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct BatcherStaticConfig {
    #[validate(nested)]
    pub storage: StorageConfig,
    pub outstream_content_buffer_size: usize,
    pub input_stream_content_buffer_size: usize,
    pub block_builder_config: BlockBuilderConfig,
    pub pre_confirmed_block_writer_config: PreconfirmedBlockWriterConfig,
    #[validate(nested)]
    pub contract_class_manager_config: ContractClassManagerConfig,
    pub commitment_manager_config: CommitmentManagerConfig,
    pub max_l1_handler_txs_per_block_proposal: usize,
    pub pre_confirmed_cende_config: PreconfirmedCendeConfig,
    pub propose_l1_txs_every: u64,
    // TODO(Amos): Move to commitment manager config.
    pub first_block_with_partial_block_hash: Option<FirstBlockWithPartialBlockHash>,
    pub storage_reader_server_static_config: StorageReaderServerStaticConfig,
    /// If true, the batcher only validates proposed blocks and cannot build proposals.
    /// Mirrors the node-level `validation_only`; `validate_node_config` asserts the two are equal.
    pub validation_only: bool,
}

impl Default for BatcherStaticConfig {
    fn default() -> Self {
        Self {
            storage: StorageConfig {
                db_config: DbConfig {
                    path_prefix: "/data/batcher".into(),
                    enforce_file_exists: false,
                    ..Default::default()
                },
                scope: StorageScope::StateOnly,
                ..Default::default()
            },
            // TODO(AlonH): set a more reasonable default value.
            outstream_content_buffer_size: 100,
            input_stream_content_buffer_size: 400,
            block_builder_config: BlockBuilderConfig::default(),
            pre_confirmed_block_writer_config: PreconfirmedBlockWriterConfig::default(),
            contract_class_manager_config: ContractClassManagerConfig::default(),
            commitment_manager_config: CommitmentManagerConfig::default(),
            max_l1_handler_txs_per_block_proposal: 3,
            pre_confirmed_cende_config: PreconfirmedCendeConfig::default(),
            propose_l1_txs_every: 1, // Default is to propose L1 transactions every proposal.
            first_block_with_partial_block_hash: None,
            storage_reader_server_static_config: StorageReaderServerStaticConfig::default(),
            validation_only: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_batcher_dynamic_config"))]
pub struct BatcherDynamicConfig {
    pub native_classes_whitelist: NativeClassesWhitelist,
    pub storage_reader_server_dynamic_config: StorageReaderServerDynamicConfig,
    /// Number of transactions in each request from the tx_provider.
    pub n_concurrent_txs: usize,
    /// Time to wait (in milliseconds) between transaction requests when the previous request
    /// returned no transactions.
    pub tx_polling_interval_millis: u64,
    /// Minimum time (in milliseconds) that must pass since block creation started before checking
    /// for idle state. If this delay has passed AND no transactions are currently being executed,
    /// the proposer will finish building the current block.
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub proposer_idle_detection_delay_millis: Duration,
}

impl Default for BatcherDynamicConfig {
    fn default() -> Self {
        Self {
            native_classes_whitelist: NativeClassesWhitelist::All,
            storage_reader_server_dynamic_config: StorageReaderServerDynamicConfig::default(),
            n_concurrent_txs: 100,
            tx_polling_interval_millis: 10,
            proposer_idle_detection_delay_millis: Duration::from_millis(1500),
        }
    }
}

/// The batcher related configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_batcher_config"))]
pub struct BatcherConfig {
    #[validate(nested)]
    pub static_config: BatcherStaticConfig,
    #[validate(nested)]
    pub dynamic_config: BatcherDynamicConfig,
}

fn validate_batcher_dynamic_config(
    dynamic_config: &BatcherDynamicConfig,
) -> Result<(), ValidationError> {
    // Idle detection delay must be > polling interval to allow time for polling to find
    // transactions.
    let idle_delay = dynamic_config.proposer_idle_detection_delay_millis;
    let polling_interval = Duration::from_millis(dynamic_config.tx_polling_interval_millis);
    if idle_delay <= polling_interval {
        return Err(ValidationError::new(
            "proposer_idle_detection_delay_millis must be greater than tx_polling_interval_millis",
        ));
    }
    Ok(())
}

fn validate_batcher_config(batcher_config: &BatcherConfig) -> Result<(), ValidationError> {
    if batcher_config.static_config.input_stream_content_buffer_size
        < batcher_config.dynamic_config.n_concurrent_txs
    {
        return Err(ValidationError::new(
            "input_stream_content_buffer_size must be at least n_concurrent_txs",
        ));
    }
    Ok(())
}
