use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::deserialize_milliseconds_to_duration;
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_sub_config,
    ser_param,
    SerializeConfig,
};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_storage::db::DbConfig;
use apollo_storage::storage_reader_server::{
    StorageReaderServerDynamicConfig,
    StorageReaderServerStaticConfig,
};
use apollo_storage::{StorageConfig, StorageScope};
use blockifier::blockifier::config::{ContractClassManagerConfig, WorkerPoolConfig};
use blockifier::blockifier_versioned_constants::VersionedConstantsOverrides;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::ChainInfo;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use url::Url;
use validator::{Validate, ValidationError};

use crate::bootstrap_config::BootstrapConfig;

pub const DEFAULT_TASKS_CHANNEL_SIZE: usize = 1000;
pub const DEFAULT_RESULTS_CHANNEL_SIZE: usize = 1000;

/// Configuration for the block builder component of the batcher.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BlockBuilderConfig {
    pub chain_info: ChainInfo,
    pub execute_config: WorkerPoolConfig,
    pub bouncer_config: BouncerConfig,
    pub n_concurrent_txs: usize,
    pub tx_polling_interval_millis: u64,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub proposer_idle_detection_delay_millis: Duration,
    pub versioned_constants_overrides: Option<VersionedConstantsOverrides>,
}

impl Default for BlockBuilderConfig {
    fn default() -> Self {
        Self {
            // TODO(AlonH): update the default values once the actual values are known.
            chain_info: ChainInfo::default(),
            execute_config: WorkerPoolConfig::default(),
            bouncer_config: BouncerConfig::default(),
            n_concurrent_txs: 100,
            tx_polling_interval_millis: 10,
            proposer_idle_detection_delay_millis: Duration::from_millis(2000),
            versioned_constants_overrides: None,
        }
    }
}

impl SerializeConfig for BlockBuilderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = prepend_sub_config_name(self.chain_info.dump(), "chain_info");
        dump.append(&mut prepend_sub_config_name(self.execute_config.dump(), "execute_config"));
        dump.append(&mut prepend_sub_config_name(self.bouncer_config.dump(), "bouncer_config"));
        dump.append(&mut BTreeMap::from([ser_param(
            "n_concurrent_txs",
            &self.n_concurrent_txs,
            "Number of transactions in each request from the tx_provider.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "tx_polling_interval_millis",
            &self.tx_polling_interval_millis,
            "Time to wait (in milliseconds) between transaction requests when the previous \
             request returned no transactions.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "proposer_idle_detection_delay_millis",
            &self.proposer_idle_detection_delay_millis.as_millis(),
            "Minimum time (in milliseconds) that must pass since block creation started before \
             checking for idle state. If this delay has passed AND no transactions are currently \
             being executed, the proposer will finish building the current block.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut ser_optional_sub_config(
            &self.versioned_constants_overrides,
            "versioned_constants_overrides",
        ));
        dump
    }
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

impl SerializeConfig for CommitmentManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "tasks_channel_size",
                &self.tasks_channel_size,
                "The size of the channel for sending tasks to the commitment manager.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "results_channel_size",
                &self.results_channel_size,
                "The size of the channel for receiving results from the commitment manager.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "panic_if_task_channel_full",
                &self.panic_if_task_channel_full,
                "If the task channel is full: if true, will panic. If false, will wait for the \
                 tasks channel to be available.",
                ParamPrivacyInput::Public,
            ),
        ])
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

impl SerializeConfig for PreconfirmedBlockWriterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "channel_buffer_capacity",
                &self.channel_buffer_capacity,
                "The capacity of the channel buffer for receiving pre-confirmed transactions.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "write_block_interval_millis",
                &self.write_block_interval_millis,
                "Time interval (ms) between writing pre-confirmed blocks. Writes occur only when \
                 block data changes.",
                ParamPrivacyInput::Public,
            ),
        ])
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

impl SerializeConfig for PreconfirmedCendeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "recorder_url",
            &self.recorder_url,
            "The URL of the Pythonic cende_recorder",
            ParamPrivacyInput::Public,
        )])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct FirstBlockWithPartialBlockHash {
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub parent_block_hash: BlockHash,
}

impl SerializeConfig for FirstBlockWithPartialBlockHash {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "block_number",
                &self.block_number,
                "The number of the first block with a partial block hash components.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "block_hash",
                &self.block_hash,
                "The hash of the first block with a partial block hash components.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "parent_block_hash",
                &self.parent_block_hash,
                "The hash of the parent block of the first block with a partial block hash \
                 components.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_batcher_static_config"))]
pub struct BatcherStaticConfig {
    pub storage: StorageConfig,
    pub outstream_content_buffer_size: usize,
    pub input_stream_content_buffer_size: usize,
    pub block_builder_config: BlockBuilderConfig,
    pub pre_confirmed_block_writer_config: PreconfirmedBlockWriterConfig,
    pub contract_class_manager_config: ContractClassManagerConfig,
    pub commitment_manager_config: CommitmentManagerConfig,
    pub max_l1_handler_txs_per_block_proposal: usize,
    pub pre_confirmed_cende_config: PreconfirmedCendeConfig,
    pub propose_l1_txs_every: u64,
    // TODO(Amos): Move to commitment manager config.
    pub first_block_with_partial_block_hash: Option<FirstBlockWithPartialBlockHash>,
    pub storage_reader_server_static_config: StorageReaderServerStaticConfig,
    /// Configuration for bootstrap mode (starting with empty storage).
    pub bootstrap_config: BootstrapConfig,
}

impl SerializeConfig for BatcherStaticConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        // TODO(yair): create nicer function to append sub configs.
        let mut dump = BTreeMap::from([
            ser_param(
                "outstream_content_buffer_size",
                &self.outstream_content_buffer_size,
                "The maximum number of items to include in a single get_proposal_content response.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "input_stream_content_buffer_size",
                &self.input_stream_content_buffer_size,
                "Sets the buffer size for the input transaction channel. Adding more transactions \
                 beyond this limit will block until space is available.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_l1_handler_txs_per_block_proposal",
                &self.max_l1_handler_txs_per_block_proposal,
                "The maximum number of L1 handler transactions to include in a block proposal.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "propose_l1_txs_every",
                &self.propose_l1_txs_every,
                "Only propose L1 transactions every N proposals.",
                ParamPrivacyInput::Public,
            ),
        ]);
        dump.append(&mut prepend_sub_config_name(self.storage.dump(), "storage"));
        dump.append(&mut prepend_sub_config_name(
            self.block_builder_config.dump(),
            "block_builder_config",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.pre_confirmed_block_writer_config.dump(),
            "pre_confirmed_block_writer_config",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.contract_class_manager_config.dump(),
            "contract_class_manager_config",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.commitment_manager_config.dump(),
            "commitment_manager_config",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.pre_confirmed_cende_config.dump(),
            "pre_confirmed_cende_config",
        ));
        dump.extend(ser_optional_sub_config(
            &self.first_block_with_partial_block_hash,
            "first_block_with_partial_block_hash",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.storage_reader_server_static_config.dump(),
            "storage_reader_server_static_config",
        ));
        dump.append(&mut prepend_sub_config_name(self.bootstrap_config.dump(), "bootstrap_config"));
        dump
    }
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
            bootstrap_config: BootstrapConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct BatcherDynamicConfig {
    pub storage_reader_server_dynamic_config: StorageReaderServerDynamicConfig,
}

impl SerializeConfig for BatcherDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        prepend_sub_config_name(
            self.storage_reader_server_dynamic_config.dump(),
            "storage_reader_server_dynamic_config",
        )
    }
}

/// The batcher related configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct BatcherConfig {
    #[validate(nested)]
    pub static_config: BatcherStaticConfig,
    #[validate(nested)]
    pub dynamic_config: BatcherDynamicConfig,
}

impl SerializeConfig for BatcherConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::new();
        config.extend(prepend_sub_config_name(self.static_config.dump(), "static_config"));
        config.extend(prepend_sub_config_name(self.dynamic_config.dump(), "dynamic_config"));
        config
    }
}

fn validate_batcher_static_config(
    batcher_static_config: &BatcherStaticConfig,
) -> Result<(), ValidationError> {
    if batcher_static_config.input_stream_content_buffer_size
        < batcher_static_config.block_builder_config.n_concurrent_txs
    {
        return Err(ValidationError::new(
            "input_stream_content_buffer_size must be at least n_concurrent_txs",
        ));
    }

    // Idle detection delay must be > polling interval to allow time for polling to find
    // transactions.
    let idle_delay =
        batcher_static_config.block_builder_config.proposer_idle_detection_delay_millis;
    let polling_interval = Duration::from_millis(
        batcher_static_config.block_builder_config.tx_polling_interval_millis,
    );
    if idle_delay <= polling_interval {
        return Err(ValidationError::new(
            "proposer_idle_detection_delay_millis must be greater than tx_polling_interval_millis",
        ));
    }

    Ok(())
}
