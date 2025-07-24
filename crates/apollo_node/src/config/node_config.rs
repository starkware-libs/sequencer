use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::sync::LazyLock;
use std::vec::Vec;

use apollo_batcher::config::BatcherConfig;
use apollo_batcher::VersionedConstantsOverrides;
use apollo_class_manager::config::FsClassManagerConfig;
use apollo_compile_to_casm::config::SierraCompilationConfig;
use apollo_config::dumping::{
    generate_struct_pointer,
    prepend_sub_config_name,
    ser_optional_sub_config,
    ser_pointer_target_param,
    set_pointing_param_paths,
    ConfigPointers,
    Pointers,
    SerializeConfig,
};
use apollo_config::loading::load_and_process_config;
use apollo_config::{ConfigError, ParamPath, SerializedParam};
use apollo_consensus_manager::config::ConsensusManagerConfig;
use apollo_gateway::config::GatewayConfig;
use apollo_http_server::config::HttpServerConfig;
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_l1_endpoint_monitor::monitor::L1EndpointMonitorConfig;
use apollo_l1_gas_price::l1_gas_price_provider::L1GasPriceProviderConfig;
use apollo_l1_gas_price::l1_gas_price_scraper::L1GasPriceScraperConfig;
use apollo_l1_provider::l1_scraper::L1ScraperConfig;
use apollo_l1_provider::L1ProviderConfig;
use apollo_mempool::config::MempoolConfig;
use apollo_mempool_p2p::config::MempoolP2pConfig;
use apollo_monitoring_endpoint::config::MonitoringEndpointConfig;
use apollo_reverts::RevertConfig;
use apollo_state_sync::config::StateSyncConfig;
use clap::Command;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::component_config::ComponentConfig;
use crate::config::monitoring::MonitoringConfig;
use crate::version::VERSION_FULL;

// The path of the default configuration file, provided as part of the crate.
pub const CONFIG_SCHEMA_PATH: &str = "crates/apollo_node/resources/config_schema.json";
pub const CONFIG_SECRETS_SCHEMA_PATH: &str =
    "crates/apollo_node/resources/config_secrets_schema.json";
pub(crate) const POINTER_TARGET_VALUE: &str = "PointerTarget";

// Configuration parameters that share the same value across multiple components.
pub static CONFIG_POINTERS: LazyLock<ConfigPointers> = LazyLock::new(|| {
    let mut pointers = vec![
        (
            ser_pointer_target_param(
                "chain_id",
                &POINTER_TARGET_VALUE.to_string(),
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.block_builder_config.chain_info.chain_id",
                "batcher_config.storage.db_config.chain_id",
                "consensus_manager_config.context_config.chain_id",
                "consensus_manager_config.network_config.chain_id",
                "gateway_config.chain_info.chain_id",
                "l1_scraper_config.chain_id",
                "l1_gas_price_scraper_config.chain_id",
                "mempool_p2p_config.network_config.chain_id",
                "state_sync_config.storage_config.db_config.chain_id",
                "state_sync_config.network_config.chain_id",
                "state_sync_config.rpc_config.chain_id",
            ]),
        ),
        (
            ser_pointer_target_param(
                "eth_fee_token_address",
                &POINTER_TARGET_VALUE.to_string(),
                "Address of the ETH fee token.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.block_builder_config.chain_info.fee_token_addresses.\
                 eth_fee_token_address",
                "gateway_config.chain_info.fee_token_addresses.eth_fee_token_address",
                "state_sync_config.rpc_config.execution_config.eth_fee_contract_address",
            ]),
        ),
        (
            ser_pointer_target_param(
                "starknet_url",
                &POINTER_TARGET_VALUE.to_string(),
                "URL for communicating with Starknet.",
            ),
            set_pointing_param_paths(&[
                "state_sync_config.central_sync_client_config.central_source_config.starknet_url",
                "state_sync_config.rpc_config.starknet_url",
            ]),
        ),
        (
            ser_pointer_target_param(
                "strk_fee_token_address",
                &POINTER_TARGET_VALUE.to_string(),
                "Address of the STRK fee token.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.block_builder_config.chain_info.fee_token_addresses.\
                 strk_fee_token_address",
                "gateway_config.chain_info.fee_token_addresses.strk_fee_token_address",
                "state_sync_config.rpc_config.execution_config.strk_fee_contract_address",
            ]),
        ),
        (
            ser_pointer_target_param(
                "validator_id",
                &POINTER_TARGET_VALUE.to_string(),
                "The ID of the validator. \
                 Also the address of this validator as a starknet contract.",
            ),
            set_pointing_param_paths(&["consensus_manager_config.consensus_manager_config.validator_id"]),
        ),
        (
            ser_pointer_target_param(
                "recorder_url",
                &POINTER_TARGET_VALUE.to_string(),
                "The URL of the Pythonic cende_recorder",
            ),
            set_pointing_param_paths(&[
                "consensus_manager_config.cende_config.recorder_url",
                "batcher_config.pre_confirmed_cende_config.recorder_url",
            ]),
        ),
        (
            ser_pointer_target_param(
                "validate_resource_bounds",
                &true,
                "Indicates that validations related to resource bounds are applied. \
                It should be set to false during a system bootstrap.",
            ),
            set_pointing_param_paths(&[
                "gateway_config.stateful_tx_validator_config.validate_resource_bounds",
                "gateway_config.stateless_tx_validator_config.validate_resource_bounds",
                "mempool_config.validate_resource_bounds",
            ]),
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
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct SequencerNodeConfig {
    // Infra related configs.
    #[validate]
    pub components: ComponentConfig,
    #[validate]
    pub monitoring_config: MonitoringConfig,

    // Business-logic component configs.
    #[validate]
    pub base_layer_config: Option<EthereumBaseLayerConfig>,
    #[validate]
    pub batcher_config: Option<BatcherConfig>,
    #[validate]
    pub class_manager_config: Option<FsClassManagerConfig>,
    #[validate]
    pub consensus_manager_config: Option<ConsensusManagerConfig>,
    #[validate]
    pub gateway_config: Option<GatewayConfig>,
    #[validate]
    pub http_server_config: Option<HttpServerConfig>,
    #[validate]
    pub l1_endpoint_monitor_config: Option<L1EndpointMonitorConfig>,
    #[validate]
    pub l1_gas_price_provider_config: Option<L1GasPriceProviderConfig>,
    #[validate]
    pub l1_gas_price_scraper_config: Option<L1GasPriceScraperConfig>,
    #[validate]
    pub l1_provider_config: Option<L1ProviderConfig>,
    #[validate]
    pub l1_scraper_config: Option<L1ScraperConfig>,
    #[validate]
    pub mempool_config: Option<MempoolConfig>,
    #[validate]
    pub mempool_p2p_config: Option<MempoolP2pConfig>,
    #[validate]
    pub monitoring_endpoint_config: Option<MonitoringEndpointConfig>,
    #[validate]
    pub sierra_compiler_config: Option<SierraCompilationConfig>,
    #[validate]
    pub state_sync_config: Option<StateSyncConfig>,
}

impl SerializeConfig for SequencerNodeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = vec![
            // Infra related configs.
            prepend_sub_config_name(self.components.dump(), "components"),
            prepend_sub_config_name(self.monitoring_config.dump(), "monitoring_config"),
            // Business-logic component configs.
            ser_optional_sub_config(&self.base_layer_config, "base_layer_config"),
            ser_optional_sub_config(&self.batcher_config, "batcher_config"),
            ser_optional_sub_config(&self.class_manager_config, "class_manager_config"),
            ser_optional_sub_config(&self.consensus_manager_config, "consensus_manager_config"),
            ser_optional_sub_config(&self.gateway_config, "gateway_config"),
            ser_optional_sub_config(&self.http_server_config, "http_server_config"),
            ser_optional_sub_config(&self.mempool_config, "mempool_config"),
            ser_optional_sub_config(&self.mempool_p2p_config, "mempool_p2p_config"),
            ser_optional_sub_config(&self.monitoring_endpoint_config, "monitoring_endpoint_config"),
            ser_optional_sub_config(&self.l1_endpoint_monitor_config, "l1_endpoint_monitor_config"),
            ser_optional_sub_config(
                &self.l1_gas_price_provider_config,
                "l1_gas_price_provider_config",
            ),
            ser_optional_sub_config(
                &self.l1_gas_price_scraper_config,
                "l1_gas_price_scraper_config",
            ),
            ser_optional_sub_config(&self.l1_provider_config, "l1_provider_config"),
            ser_optional_sub_config(&self.l1_scraper_config, "l1_scraper_config"),
            ser_optional_sub_config(&self.sierra_compiler_config, "sierra_compiler_config"),
            ser_optional_sub_config(&self.state_sync_config, "state_sync_config"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}

impl Default for SequencerNodeConfig {
    fn default() -> Self {
        Self {
            // Infra related configs.
            components: ComponentConfig::default(),
            monitoring_config: MonitoringConfig::default(),
            // Business-logic component configs.
            base_layer_config: Some(EthereumBaseLayerConfig::default()),
            batcher_config: Some(BatcherConfig::default()),
            class_manager_config: Some(FsClassManagerConfig::default()),
            consensus_manager_config: Some(ConsensusManagerConfig::default()),
            gateway_config: Some(GatewayConfig::default()),
            http_server_config: Some(HttpServerConfig::default()),
            l1_endpoint_monitor_config: Some(L1EndpointMonitorConfig::default()),
            l1_gas_price_provider_config: Some(L1GasPriceProviderConfig::default()),
            l1_gas_price_scraper_config: Some(L1GasPriceScraperConfig::default()),
            l1_provider_config: Some(L1ProviderConfig::default()),
            l1_scraper_config: Some(L1ScraperConfig::default()),
            mempool_config: Some(MempoolConfig::default()),
            mempool_p2p_config: Some(MempoolP2pConfig::default()),
            monitoring_endpoint_config: Some(MonitoringEndpointConfig::default()),
            sierra_compiler_config: Some(SierraCompilationConfig::default()),
            state_sync_config: Some(StateSyncConfig::default()),
        }
    }
}

impl SequencerNodeConfig {
    /// Creates a config object, using the config schema and provided resources.
    pub fn load_and_process(args: Vec<String>) -> Result<Self, ConfigError> {
        let config_file_name = &resolve_project_relative_path(CONFIG_SCHEMA_PATH)?;
        let default_config_file = File::open(config_file_name)?;
        load_and_process_config(default_config_file, node_command(), args, true)
    }
}

/// The command line interface of this node.
pub(crate) fn node_command() -> Command {
    Command::new("Sequencer")
        .version(VERSION_FULL)
        .about("A Starknet sequencer node written in Rust.")
}
