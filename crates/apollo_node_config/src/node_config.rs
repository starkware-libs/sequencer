use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::sync::LazyLock;
use std::vec::Vec;

use apollo_batcher_config::config::{BatcherConfig, BatcherDynamicConfig};
use apollo_class_manager_config::config::FsClassManagerConfig;
use apollo_committer_config::config::ApolloCommitterConfig;
use apollo_config::dumping::{
    generate_optional_struct_pointer,
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
use apollo_config::validators::config_validate;
use apollo_config::{ConfigError, ParamPath, SerializedParam};
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_manager_config::config::ConsensusManagerConfig;
use apollo_consensus_orchestrator_config::config::ContextDynamicConfig;
use apollo_gateway_config::config::GatewayConfig;
use apollo_http_server_config::config::{HttpServerConfig, HttpServerDynamicConfig};
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_l1_gas_price_provider_config::config::{
    L1GasPriceProviderConfig,
    L1GasPriceScraperConfig,
};
use apollo_l1_provider_config::config::L1ProviderConfig;
use apollo_l1_scraper_config::config::L1ScraperConfig;
use apollo_mempool_config::config::{MempoolConfig, MempoolDynamicConfig};
use apollo_mempool_p2p_config::config::MempoolP2pConfig;
use apollo_monitoring_endpoint_config::config::MonitoringEndpointConfig;
use apollo_reverts::RevertConfig;
use apollo_sierra_compilation_config::config::SierraCompilationConfig;
use apollo_staking_config::config::StakingManagerDynamicConfig;
use apollo_state_sync_config::config::{StateSyncConfig, StateSyncDynamicConfig};
use blockifier::blockifier_versioned_constants::VersionedConstantsOverrides;
use clap::Command;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::component_config::ComponentConfig;
use crate::component_execution_config::ExpectedComponentConfig;
use crate::monitoring::MonitoringConfig;
use crate::version::VERSION_FULL;

// The path of the configuration schema file, provided as part of the crate.
pub const CONFIG_SCHEMA_PATH: &str = "crates/apollo_node/resources/config_schema.json";
pub const CONFIG_SECRETS_SCHEMA_PATH: &str =
    "crates/apollo_node/resources/config_secrets_schema.json";
pub const POINTER_TARGET_VALUE: &str = "PointerTarget";

// TODO(Tsabary): move metrics recorder to the node level, like tracing, instead of being
// initialized as part of the endpoint.

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
                "batcher_config.static_config.block_builder_config.chain_info.chain_id",
                "batcher_config.static_config.storage.db_config.chain_id",
                "class_manager_config.static_config.class_storage_config.class_hash_storage_config.db_config.chain_id",
                "consensus_manager_config.consensus_manager_config.static_config.storage_config.db_config.chain_id",
                "consensus_manager_config.context_config.static_config.chain_id",
                "consensus_manager_config.network_config.chain_id",
                "gateway_config.static_config.chain_info.chain_id",
                "l1_scraper_config.chain_id",
                "l1_gas_price_scraper_config.chain_id",
                "mempool_p2p_config.network_config.chain_id",
                "state_sync_config.static_config.storage_config.db_config.chain_id",
                "state_sync_config.static_config.network_config.chain_id",
                "state_sync_config.static_config.rpc_config.chain_id",
            ]),
        ),
        (
            ser_pointer_target_param(
                "eth_fee_token_address",
                &POINTER_TARGET_VALUE.to_string(),
                "Address of the ETH fee token.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.static_config.block_builder_config.chain_info.fee_token_addresses.\
                 eth_fee_token_address",
                "gateway_config.static_config.chain_info.fee_token_addresses.eth_fee_token_address",
                "state_sync_config.static_config.rpc_config.execution_config.eth_fee_contract_address",
            ]),
        ),
        (
            ser_pointer_target_param(
                "native_classes_whitelist",
                &"[]".to_string(),
                "Specifies whether to execute all class hashes or only specific ones using Cairo \
                native. If limited, a specific list of class hashes is provided.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.static_config.contract_class_manager_config.cairo_native_run_config.\
                native_classes_whitelist",
                "gateway_config.static_config.contract_class_manager_config.cairo_native_run_config.\
                native_classes_whitelist",
            ]),
        ),
        (
            ser_pointer_target_param(
                "starknet_url",
                &POINTER_TARGET_VALUE.to_string(),
                "URL for communicating with Starknet.",
            ),
            set_pointing_param_paths(&[
                "state_sync_config.static_config.central_sync_client_config.central_source_config.starknet_url",
                "state_sync_config.static_config.rpc_config.starknet_url",
            ]),
        ),
        (
            ser_pointer_target_param(
                "strk_fee_token_address",
                &POINTER_TARGET_VALUE.to_string(),
                "Address of the STRK fee token.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.static_config.block_builder_config.chain_info.fee_token_addresses.\
                 strk_fee_token_address",
                "gateway_config.static_config.chain_info.fee_token_addresses.strk_fee_token_address",
                "state_sync_config.static_config.rpc_config.execution_config.strk_fee_contract_address",
            ]),
        ),
        (
            ser_pointer_target_param(
                "validator_id",
                &POINTER_TARGET_VALUE.to_string(),
                "The ID of the validator. \
                 Also the address of this validator as a starknet contract.",
            ),
            set_pointing_param_paths(&["consensus_manager_config.consensus_manager_config.dynamic_config.validator_id"]),
        ),
        (
            ser_pointer_target_param(
                "recorder_url",
                &POINTER_TARGET_VALUE.to_string(),
                "The URL of the Pythonic cende_recorder",
            ),
            set_pointing_param_paths(&[
                "consensus_manager_config.cende_config.recorder_url",
                "batcher_config.static_config.pre_confirmed_cende_config.recorder_url",
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
                "gateway_config.static_config.stateful_tx_validator_config.validate_resource_bounds",
                "gateway_config.static_config.stateless_tx_validator_config.validate_resource_bounds",
                "mempool_config.static_config.validate_resource_bounds",
            ]),
        ),
    ];
    let mut common_execution_config = generate_optional_struct_pointer::<VersionedConstantsOverrides>(
        "versioned_constants_overrides".to_owned(),
        None,
        set_pointing_param_paths(&[
            "batcher_config.static_config.block_builder_config.versioned_constants_overrides",
            "gateway_config.static_config.stateful_tx_validator_config.\
             versioned_constants_overrides",
        ]),
    );
    pointers.append(&mut common_execution_config);

    let mut common_execution_config = generate_struct_pointer(
        "revert_config".to_owned(),
        &RevertConfig::default(),
        set_pointing_param_paths(&[
            "state_sync_config.static_config.revert_config",
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
    #[validate(nested)]
    pub components: ComponentConfig,
    #[validate(nested)]
    pub config_manager_config: Option<ConfigManagerConfig>,
    #[validate(nested)]
    pub monitoring_config: MonitoringConfig,
    // Business-logic component configs.
    #[validate(nested)]
    pub base_layer_config: Option<EthereumBaseLayerConfig>,
    #[validate(nested)]
    pub batcher_config: Option<BatcherConfig>,
    #[validate(nested)]
    pub class_manager_config: Option<FsClassManagerConfig>,
    #[validate(nested)]
    pub committer_config: Option<ApolloCommitterConfig>,
    #[validate(nested)]
    pub consensus_manager_config: Option<ConsensusManagerConfig>,
    #[validate(nested)]
    pub gateway_config: Option<GatewayConfig>,
    #[validate(nested)]
    pub http_server_config: Option<HttpServerConfig>,
    #[validate(nested)]
    pub l1_gas_price_provider_config: Option<L1GasPriceProviderConfig>,
    #[validate(nested)]
    pub l1_gas_price_scraper_config: Option<L1GasPriceScraperConfig>,
    #[validate(nested)]
    pub l1_provider_config: Option<L1ProviderConfig>,
    #[validate(nested)]
    pub l1_scraper_config: Option<L1ScraperConfig>,
    #[validate(nested)]
    pub mempool_config: Option<MempoolConfig>,
    #[validate(nested)]
    pub mempool_p2p_config: Option<MempoolP2pConfig>,
    #[validate(nested)]
    pub monitoring_endpoint_config: Option<MonitoringEndpointConfig>,
    #[validate(nested)]
    pub sierra_compiler_config: Option<SierraCompilationConfig>,
    #[validate(nested)]
    pub state_sync_config: Option<StateSyncConfig>,
}

impl SerializeConfig for SequencerNodeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = vec![
            // Infra related configs.
            prepend_sub_config_name(self.components.dump(), "components"),
            ser_optional_sub_config(&self.config_manager_config, "config_manager_config"),
            prepend_sub_config_name(self.monitoring_config.dump(), "monitoring_config"),
            // Business-logic component configs.
            ser_optional_sub_config(&self.base_layer_config, "base_layer_config"),
            ser_optional_sub_config(&self.batcher_config, "batcher_config"),
            ser_optional_sub_config(&self.class_manager_config, "class_manager_config"),
            ser_optional_sub_config(&self.committer_config, "committer_config"),
            ser_optional_sub_config(&self.consensus_manager_config, "consensus_manager_config"),
            ser_optional_sub_config(&self.gateway_config, "gateway_config"),
            ser_optional_sub_config(&self.http_server_config, "http_server_config"),
            ser_optional_sub_config(&self.mempool_config, "mempool_config"),
            ser_optional_sub_config(&self.mempool_p2p_config, "mempool_p2p_config"),
            ser_optional_sub_config(&self.monitoring_endpoint_config, "monitoring_endpoint_config"),
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
            config_manager_config: Some(ConfigManagerConfig::default()),
            monitoring_config: MonitoringConfig::default(),
            // Business-logic component configs.
            base_layer_config: Some(EthereumBaseLayerConfig::default()),
            batcher_config: Some(BatcherConfig::default()),
            class_manager_config: Some(FsClassManagerConfig::default()),
            committer_config: Some(ApolloCommitterConfig::default()),
            consensus_manager_config: Some(ConsensusManagerConfig::default()),
            gateway_config: Some(GatewayConfig::default()),
            http_server_config: Some(HttpServerConfig::default()),
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

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate, Default)]
pub struct NodeDynamicConfig {
    #[validate(nested)]
    pub batcher_dynamic_config: Option<BatcherDynamicConfig>,
    #[validate(nested)]
    pub consensus_dynamic_config: Option<ConsensusDynamicConfig>,
    #[validate(nested)]
    pub context_dynamic_config: Option<ContextDynamicConfig>,
    #[validate(nested)]
    pub http_server_dynamic_config: Option<HttpServerDynamicConfig>,
    #[validate(nested)]
    pub mempool_dynamic_config: Option<MempoolDynamicConfig>,
    #[validate(nested)]
    pub staking_manager_dynamic_config: Option<StakingManagerDynamicConfig>,
    #[validate(nested)]
    pub state_sync_dynamic_config: Option<StateSyncDynamicConfig>,
}

impl SerializeConfig for NodeDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = [
            ser_optional_sub_config(&self.batcher_dynamic_config, "batcher_dynamic_config"),
            ser_optional_sub_config(&self.consensus_dynamic_config, "consensus_dynamic_config"),
            ser_optional_sub_config(&self.context_dynamic_config, "context_dynamic_config"),
            ser_optional_sub_config(&self.http_server_dynamic_config, "http_server_dynamic_config"),
            ser_optional_sub_config(&self.mempool_dynamic_config, "mempool_dynamic_config"),
            ser_optional_sub_config(
                &self.staking_manager_dynamic_config,
                "staking_manager_dynamic_config",
            ),
            ser_optional_sub_config(&self.state_sync_dynamic_config, "state_sync_dynamic_config"),
        ];
        sub_configs.into_iter().flatten().collect()
    }
}

impl From<&SequencerNodeConfig> for NodeDynamicConfig {
    fn from(sequencer_node_config: &SequencerNodeConfig) -> Self {
        // TODO(Nadin/Tsabary): consider creating a macro for this.
        let batcher_dynamic_config = sequencer_node_config
            .batcher_config
            .as_ref()
            .map(|batcher_config| batcher_config.dynamic_config.clone());
        let consensus_dynamic_config = sequencer_node_config.consensus_manager_config.as_ref().map(
            |consensus_manager_config| {
                consensus_manager_config.consensus_manager_config.dynamic_config.clone()
            },
        );
        let context_dynamic_config = sequencer_node_config.consensus_manager_config.as_ref().map(
            |consensus_manager_config| {
                consensus_manager_config.context_config.dynamic_config.clone()
            },
        );
        let http_server_dynamic_config = sequencer_node_config
            .http_server_config
            .as_ref()
            .map(|http_server_config| http_server_config.dynamic_config.clone());
        let mempool_dynamic_config = sequencer_node_config
            .mempool_config
            .as_ref()
            .map(|mempool_config| mempool_config.dynamic_config.clone());
        let staking_manager_dynamic_config = sequencer_node_config
            .consensus_manager_config
            .as_ref()
            .map(|consensus_manager_config| {
                consensus_manager_config.staking_manager_config.dynamic_config.clone()
            });
        let state_sync_dynamic_config = sequencer_node_config
            .state_sync_config
            .as_ref()
            .map(|state_sync_config| state_sync_config.dynamic_config.clone());
        Self {
            batcher_dynamic_config,
            consensus_dynamic_config,
            context_dynamic_config,
            http_server_dynamic_config,
            mempool_dynamic_config,
            staking_manager_dynamic_config,
            state_sync_dynamic_config,
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

    pub fn validate_node_config(&self) -> Result<(), ConfigError> {
        // Validate each config member using its `Validate` trait derivation.
        config_validate(self)?;

        // Custom cross member validations.
        self.cross_member_validations()
    }

    fn cross_member_validations(&self) -> Result<(), ConfigError> {
        macro_rules! validate_component_config_is_set_iff_running_locally {
            ($component_field:ident, $config_field:ident) => {{
                // The component config should be set iff its running locally.
                if self.components.$component_field.is_running_locally()
                    != self.$config_field.is_some()
                {
                    let execution_mode = &self.components.$component_field.execution_mode;
                    let component_config_availability =
                        if self.$config_field.is_some() { "available" } else { "not available" };
                    return Err(ConfigError::ComponentConfigMismatch {
                        component_config_mismatch: format!(
                            "{} component configs mismatch: execution mode {:?} while config is {}",
                            stringify!($component_field),
                            execution_mode,
                            component_config_availability
                        ),
                    });
                }
            }};
        }

        // TODO(Tsabary): should be based on iteration of `ComponentConfig` fields.
        validate_component_config_is_set_iff_running_locally!(batcher, batcher_config);
        validate_component_config_is_set_iff_running_locally!(class_manager, class_manager_config);
        validate_component_config_is_set_iff_running_locally!(committer, committer_config);
        validate_component_config_is_set_iff_running_locally!(
            config_manager,
            config_manager_config
        );
        validate_component_config_is_set_iff_running_locally!(
            consensus_manager,
            consensus_manager_config
        );
        validate_component_config_is_set_iff_running_locally!(gateway, gateway_config);
        validate_component_config_is_set_iff_running_locally!(http_server, http_server_config);
        validate_component_config_is_set_iff_running_locally!(
            l1_gas_price_provider,
            l1_gas_price_provider_config
        );
        validate_component_config_is_set_iff_running_locally!(
            l1_gas_price_scraper,
            l1_gas_price_scraper_config
        );
        validate_component_config_is_set_iff_running_locally!(l1_provider, l1_provider_config);
        validate_component_config_is_set_iff_running_locally!(l1_scraper, l1_scraper_config);
        validate_component_config_is_set_iff_running_locally!(mempool, mempool_config);
        validate_component_config_is_set_iff_running_locally!(mempool_p2p, mempool_p2p_config);
        validate_component_config_is_set_iff_running_locally!(
            monitoring_endpoint,
            monitoring_endpoint_config
        );
        validate_component_config_is_set_iff_running_locally!(
            sierra_compiler,
            sierra_compiler_config
        );
        validate_component_config_is_set_iff_running_locally!(state_sync, state_sync_config);

        // Validate proposer_idle_detection_delay < batcher_deadline.
        // The batcher_deadline = proposal_timeout - build_proposal_margin.
        // If idle_delay >= batcher_deadline, idle detection never triggers (hard deadline fires
        // first).
        if let (Some(batcher_config), Some(consensus_manager_config)) =
            (&self.batcher_config, &self.consensus_manager_config)
        {
            let idle_delay = batcher_config
                .static_config
                .block_builder_config
                .proposer_idle_detection_delay_millis;
            let proposal_timeout = consensus_manager_config
                .consensus_manager_config
                .dynamic_config
                .timeouts
                .get_proposal_timeout(0); // base timeout (round 0)
            let build_margin =
                consensus_manager_config.context_config.static_config.build_proposal_margin_millis;
            let batcher_deadline = proposal_timeout.saturating_sub(build_margin);

            if idle_delay >= batcher_deadline {
                return Err(ConfigError::ComponentConfigMismatch {
                    component_config_mismatch: format!(
                        "proposer_idle_detection_delay_millis ({:?}) must be less than \
                         batcher_deadline ({:?}) = proposal_timeout ({:?}) - build_margin ({:?})",
                        idle_delay, batcher_deadline, proposal_timeout, build_margin
                    ),
                });
            }
        }

        Ok(())
    }
}

/// The command line interface of this node.
pub fn node_command() -> Command {
    Command::new("Sequencer")
        .version(VERSION_FULL)
        .about("A Starknet sequencer node written in Rust.")
}
