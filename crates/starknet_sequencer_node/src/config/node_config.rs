use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::LazyLock;
use std::vec::Vec;

use apollo_reverts::RevertConfig;
use clap::Command;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_config::dumping::{
    append_sub_config_name,
    generate_struct_pointer,
    ser_pointer_target_required_param,
    set_pointing_param_paths,
    ConfigPointers,
    Pointers,
    SerializeConfig,
};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::{ConfigError, ParamPath, SerializationType, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_batcher::config::BatcherConfig;
use starknet_batcher::VersionedConstantsOverrides;
use starknet_class_manager::config::FsClassManagerConfig;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway::config::GatewayConfig;
use starknet_http_server::config::HttpServerConfig;
use starknet_infra_utils::path::resolve_project_relative_path;
use starknet_l1_provider::l1_scraper::L1ScraperConfig;
use starknet_l1_provider::L1ProviderConfig;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_sierra_multicompile::config::SierraCompilationConfig;
use starknet_state_sync::config::StateSyncConfig;
use validator::Validate;

use crate::config::component_config::ComponentConfig;
use crate::version::VERSION_FULL;

// The path of the default configuration file, provided as part of the crate.
pub const DEFAULT_CONFIG_PATH: &str = "config/sequencer/default_config.json";
pub const DEFAULT_PRESET_CONFIG_PATH: &str = "config/sequencer/presets/config.json";

// Configuration parameters that share the same value across multiple components.
pub static CONFIG_POINTERS: LazyLock<ConfigPointers> = LazyLock::new(|| {
    let mut pointers = vec![
        (
            ser_pointer_target_required_param(
                "chain_id",
                SerializationType::String,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.block_builder_config.chain_info.chain_id",
                "batcher_config.storage.db_config.chain_id",
                "consensus_manager_config.context_config.chain_id",
                "consensus_manager_config.network_config.chain_id",
                "gateway_config.chain_info.chain_id",
                "l1_scraper_config.chain_id",
                "mempool_p2p_config.network_config.chain_id",
                "state_sync_config.storage_config.db_config.chain_id",
                "state_sync_config.network_config.chain_id",
            ]),
        ),
        (
            ser_pointer_target_required_param(
                "eth_fee_token_address",
                SerializationType::String,
                "Address of the ETH fee token.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.block_builder_config.chain_info.fee_token_addresses.\
                 eth_fee_token_address",
                "gateway_config.chain_info.fee_token_addresses.eth_fee_token_address",
            ]),
        ),
        (
            ser_pointer_target_required_param(
                "strk_fee_token_address",
                SerializationType::String,
                "Address of the STRK fee token.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.block_builder_config.chain_info.fee_token_addresses.\
                 strk_fee_token_address",
                "gateway_config.chain_info.fee_token_addresses.strk_fee_token_address",
            ]),
        ),
        (
            ser_pointer_target_required_param(
                "validator_id",
                SerializationType::String,
                "The ID of the validator. \
                 Also the address of this validator as a starknet contract.",
            ),
            set_pointing_param_paths(&["consensus_manager_config.consensus_config.validator_id"]),
        ),
        (
            ser_pointer_target_required_param(
                "recorder_url",
                SerializationType::String,
                "The URL of the Pythonic cende_recorder",
            ),
            set_pointing_param_paths(&["consensus_manager_config.cende_config.recorder_url"]),
        ),
    ];
    let mut common_execution_config = generate_struct_pointer(
        "versioned_constants_overrides".to_owned(),
        &VersionedConstantsOverrides::default(),
        set_pointing_param_paths(&[
            "batcher_config.block_builder_config.versioned_constants_overrides",
            "gateway_config.stateful_tx_validator_config.versioned_constants_overrides",
        ]),
    );
    pointers.append(&mut common_execution_config);

    let mut common_execution_config = generate_struct_pointer(
        "revert_config".to_owned(),
        &RevertConfig::default(),
        set_pointing_param_paths(&[
            "state_sync_config.revert_config",
            "consensus_manager_config.revert_config",
        ]),
    );
    pointers.append(&mut common_execution_config);
    pointers
});

// Parameters that should 1) not be pointers, and 2) have a name matching a pointer target param.
pub static CONFIG_NON_POINTERS_WHITELIST: LazyLock<Pointers> =
    LazyLock::new(HashSet::<ParamPath>::new);

/// The configurations of the various components of the node.
#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct SequencerNodeConfig {
    #[validate]
    pub components: ComponentConfig,
    #[validate]
    pub base_layer_config: EthereumBaseLayerConfig,
    #[validate]
    pub batcher_config: BatcherConfig,
    #[validate]
    pub class_manager_config: FsClassManagerConfig,
    #[validate]
    pub consensus_manager_config: ConsensusManagerConfig,
    #[validate]
    pub gateway_config: GatewayConfig,
    #[validate]
    pub http_server_config: HttpServerConfig,
    #[validate]
    pub compiler_config: SierraCompilationConfig,
    #[validate]
    pub l1_provider_config: L1ProviderConfig,
    #[validate]
    pub l1_scraper_config: L1ScraperConfig,
    #[validate]
    pub mempool_p2p_config: MempoolP2pConfig,
    #[validate]
    pub monitoring_endpoint_config: MonitoringEndpointConfig,
    #[validate]
    pub state_sync_config: StateSyncConfig,
}

impl SerializeConfig for SequencerNodeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = vec![
            append_sub_config_name(self.components.dump(), "components"),
            append_sub_config_name(self.base_layer_config.dump(), "base_layer_config"),
            append_sub_config_name(self.batcher_config.dump(), "batcher_config"),
            append_sub_config_name(self.class_manager_config.dump(), "class_manager_config"),
            append_sub_config_name(
                self.consensus_manager_config.dump(),
                "consensus_manager_config",
            ),
            append_sub_config_name(self.gateway_config.dump(), "gateway_config"),
            append_sub_config_name(self.http_server_config.dump(), "http_server_config"),
            append_sub_config_name(self.compiler_config.dump(), "compiler_config"),
            append_sub_config_name(self.mempool_p2p_config.dump(), "mempool_p2p_config"),
            append_sub_config_name(
                self.monitoring_endpoint_config.dump(),
                "monitoring_endpoint_config",
            ),
            append_sub_config_name(self.state_sync_config.dump(), "state_sync_config"),
            append_sub_config_name(self.l1_provider_config.dump(), "l1_provider_config"),
            append_sub_config_name(self.l1_scraper_config.dump(), "l1_scraper_config"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}

impl SequencerNodeConfig {
    /// Creates a config object. Selects the values from the default file and from resources with
    /// higher priority.
    fn load_and_process_config_file(
        args: Vec<String>,
        config_file_name: Option<&str>,
    ) -> Result<Self, ConfigError> {
        let config_file_name = match config_file_name {
            Some(file_name) => Path::new(file_name),
            None => &resolve_project_relative_path(DEFAULT_CONFIG_PATH)?,
        };

        let default_config_file = File::open(config_file_name)?;
        load_and_process_config(default_config_file, node_command(), args)
    }

    pub fn load_and_process(args: Vec<String>) -> Result<Self, ConfigError> {
        Self::load_and_process_config_file(args, None)
    }
    pub fn load_and_process_file(
        args: Vec<String>,
        config_file_name: &str,
    ) -> Result<Self, ConfigError> {
        Self::load_and_process_config_file(args, Some(config_file_name))
    }
}

/// The command line interface of this node.
pub(crate) fn node_command() -> Command {
    Command::new("Sequencer")
        .version(VERSION_FULL)
        .about("A Starknet sequencer node written in Rust.")
}
