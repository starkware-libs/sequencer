use std::vec::Vec;

use apollo_batcher_config::config::{BatcherConfig, BatcherDynamicConfig};
use apollo_class_manager_config::config::{ClassManagerDynamicConfig, FsClassManagerConfig};
use apollo_committer_config::config::ApolloCommitterConfig;
use apollo_config::loading::load_and_process_config;
use apollo_config::validators::config_validate;
use apollo_config::ConfigError;
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_manager_config::config::ConsensusManagerConfig;
use apollo_consensus_orchestrator_config::config::ContextDynamicConfig;
use apollo_gateway_config::config::{GatewayConfig, GatewayDynamicConfig};
use apollo_http_server_config::config::{HttpServerConfig, HttpServerDynamicConfig};
use apollo_l1_events_config::config::{L1EventsProviderConfig, L1EventsScraperConfig};
use apollo_l1_gas_price_config::config::{L1GasPriceProviderConfig, L1GasPriceScraperConfig};
use apollo_mempool_config::config::{MempoolConfig, MempoolDynamicConfig};
use apollo_mempool_p2p_config::config::MempoolP2pConfig;
use apollo_monitoring_endpoint_config::config::MonitoringEndpointConfig;
use apollo_proof_manager_config::config::ProofManagerConfig;
use apollo_sierra_compilation_config::config::SierraCompilationConfig;
use apollo_staking_config::config::StakingManagerDynamicConfig;
use apollo_state_sync_config::config::{StateSyncConfig, StateSyncDynamicConfig};
use clap::Command;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::component_config::{ComponentConfig, ValidateTxIngestionComponentsDisabled};
use crate::component_execution_config::{ExpectedComponentConfig, ReactiveComponentExecutionMode};
use crate::monitoring::MonitoringConfig;
use crate::version::VERSION_FULL;

// The path of the secrets schema file (the serialized private-parameter set), provided as part of
// the crate.
pub const CONFIG_SECRETS_SCHEMA_PATH: &str =
    "crates/apollo_node/resources/config_secrets_schema.json";

// TODO(Tsabary): move metrics recorder to the node level, like tracing, instead of being
// initialized as part of the endpoint.

/// The configurations of the various components of the node.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct SequencerNodeConfig {
    /// If true, the node validates proposed blocks but does not build proposals.
    /// Requires gateway, http_server, and mempool to be disabled.
    pub validation_only: bool,
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
    pub l1_events_provider_config: Option<L1EventsProviderConfig>,
    #[validate(nested)]
    pub l1_events_scraper_config: Option<L1EventsScraperConfig>,
    #[validate(nested)]
    pub mempool_config: Option<MempoolConfig>,
    #[validate(nested)]
    pub mempool_p2p_config: Option<MempoolP2pConfig>,
    #[validate(nested)]
    pub monitoring_endpoint_config: Option<MonitoringEndpointConfig>,
    #[validate(nested)]
    pub proof_manager_config: Option<ProofManagerConfig>,
    #[validate(nested)]
    pub sierra_compiler_config: Option<SierraCompilationConfig>,
    #[validate(nested)]
    pub state_sync_config: Option<StateSyncConfig>,
}

impl Default for SequencerNodeConfig {
    fn default() -> Self {
        Self {
            validation_only: false,
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
            l1_events_provider_config: Some(L1EventsProviderConfig::default()),
            l1_events_scraper_config: Some(L1EventsScraperConfig::default()),
            mempool_config: Some(MempoolConfig::default()),
            mempool_p2p_config: Some(MempoolP2pConfig::default()),
            monitoring_endpoint_config: Some(MonitoringEndpointConfig::default()),
            proof_manager_config: Some(ProofManagerConfig::default()),
            sierra_compiler_config: Some(SierraCompilationConfig::default()),
            state_sync_config: Some(StateSyncConfig::default()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate, Default)]
#[validate(schema(function = "validate_node_dynamic_config"))]
pub struct NodeDynamicConfig {
    #[validate(nested)]
    pub batcher_dynamic_config: Option<BatcherDynamicConfig>,
    #[validate(nested)]
    pub class_manager_dynamic_config: Option<ClassManagerDynamicConfig>,
    #[validate(nested)]
    pub consensus_dynamic_config: Option<ConsensusDynamicConfig>,
    #[validate(nested)]
    pub context_dynamic_config: Option<ContextDynamicConfig>,
    #[validate(nested)]
    pub gateway_dynamic_config: Option<GatewayDynamicConfig>,
    #[validate(nested)]
    pub http_server_dynamic_config: Option<HttpServerDynamicConfig>,
    #[validate(nested)]
    pub mempool_dynamic_config: Option<MempoolDynamicConfig>,
    #[validate(nested)]
    pub staking_manager_dynamic_config: Option<StakingManagerDynamicConfig>,
    #[validate(nested)]
    pub state_sync_dynamic_config: Option<StateSyncDynamicConfig>,
}

impl From<&SequencerNodeConfig> for NodeDynamicConfig {
    fn from(sequencer_node_config: &SequencerNodeConfig) -> Self {
        // TODO(Nadin/Tsabary): consider creating a macro for this.
        let batcher_dynamic_config = sequencer_node_config
            .batcher_config
            .as_ref()
            .map(|batcher_config| batcher_config.dynamic_config.clone());
        let class_manager_dynamic_config = sequencer_node_config
            .class_manager_config
            .as_ref()
            .map(|class_manager_config| class_manager_config.dynamic_config.clone());
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
        let gateway_dynamic_config = sequencer_node_config
            .gateway_config
            .as_ref()
            .map(|gateway_config| gateway_config.dynamic_config.clone());
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
            class_manager_dynamic_config,
            consensus_dynamic_config,
            context_dynamic_config,
            gateway_dynamic_config,
            http_server_dynamic_config,
            mempool_dynamic_config,
            staking_manager_dynamic_config,
            state_sync_dynamic_config,
        }
    }
}

fn validate_node_dynamic_config(config: &NodeDynamicConfig) -> Result<(), ValidationError> {
    let (Some(consensus), Some(context)) =
        (&config.consensus_dynamic_config, &config.context_dynamic_config)
    else {
        return Ok(());
    };
    let min_timeout = consensus.timeouts.get_proposal_timeout(0);
    let margin = context.build_proposal_margin_millis;
    if margin >= min_timeout {
        return Err(ValidationError::new(
            "build_proposal_margin_millis must be less than the base proposal timeout",
        ));
    }
    Ok(())
}

impl SequencerNodeConfig {
    /// Creates a config object from the native config files named by `args`.
    pub fn load_and_process(args: Vec<String>) -> Result<Self, ConfigError> {
        load_and_process_config(node_command(), args, true)
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
        validate_component_config_is_set_iff_running_locally!(
            l1_events_provider,
            l1_events_provider_config
        );
        validate_component_config_is_set_iff_running_locally!(
            l1_events_scraper,
            l1_events_scraper_config
        );
        validate_component_config_is_set_iff_running_locally!(mempool, mempool_config);
        validate_component_config_is_set_iff_running_locally!(mempool_p2p, mempool_p2p_config);
        validate_component_config_is_set_iff_running_locally!(
            monitoring_endpoint,
            monitoring_endpoint_config
        );
        validate_component_config_is_set_iff_running_locally!(proof_manager, proof_manager_config);
        validate_component_config_is_set_iff_running_locally!(
            sierra_compiler,
            sierra_compiler_config
        );
        validate_component_config_is_set_iff_running_locally!(state_sync, state_sync_config);

        // The config manager is a local infrastructure component: every node runs its own instance
        // and its consumers (e.g. the mempool) reach it over an in-process channel. Allowing it to
        // run remotely would turn the per-request `get_*_dynamic_config` calls into network RPCs,
        // where a transient failure would crash the consuming component. Enforce here that it is
        // never exposed over (or reached over) the network so that client is always local.
        match self.components.config_manager.execution_mode {
            ReactiveComponentExecutionMode::Remote
            | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                return Err(ConfigError::ComponentConfigMismatch {
                    component_config_mismatch: format!(
                        "config_manager must run locally without a remote server (execution mode \
                         must be Disabled or LocalExecutionWithRemoteDisabled), but is {:?}. It \
                         is a local infrastructure component and must be co-located with its \
                         consumers.",
                        self.components.config_manager.execution_mode
                    ),
                });
            }
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ReactiveComponentExecutionMode::Disabled => {}
        }

        // Validate proposer_idle_detection_delay < batcher_deadline.
        // The batcher_deadline = proposal_timeout - build_proposal_margin.
        // If idle_delay >= batcher_deadline, idle detection never triggers (hard deadline fires
        // first).
        if let (Some(batcher_config), Some(consensus_manager_config)) =
            (&self.batcher_config, &self.consensus_manager_config)
        {
            let idle_delay = batcher_config.dynamic_config.proposer_idle_detection_delay_millis;
            let proposal_timeout = consensus_manager_config
                .consensus_manager_config
                .dynamic_config
                .timeouts
                .get_proposal_timeout(0); // base timeout (round 0)
            let build_margin =
                consensus_manager_config.context_config.dynamic_config.build_proposal_margin_millis;
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

        self.validate_validation_only_config()?;

        self.validate_pointer_groups_equal()?;

        Ok(())
    }

    /// Asserts that the formerly-pointer-resolved values are equal across all present components.
    ///
    /// The pointer mechanism copied a single source value into many nested component fields at load
    /// time. Once those values are baked into the config (e.g. by jsonnet `build()`) nothing
    /// re-checks that the copies actually agree. These present-only equality asserts are a
    /// defense-in-depth guard for hand-edited, test, or otherwise non-jsonnet-generated configs:
    /// for each pointer group with more than one target, every present component must hold the same
    /// value. Components that are absent (`None`) are skipped, so partial/distributed deployments
    /// that own only a subset of a group still validate. Each group below mirrors one multi-target
    /// pointer group, plus `validation_only`: a single-target pointer whose target (the batcher's
    /// copy) is checked against the always-present top-level source field, since the node reads the
    /// two independently. Two single-target pointers are intentionally not checked here:
    /// `starknet_url` (its targets are typed `String` vs `Url` and cannot be compared directly) and
    /// `validator_id` (a lone target with no independent source to disagree with).
    fn validate_pointer_groups_equal(&self) -> Result<(), ConfigError> {
        let batcher = self.batcher_config.as_ref();
        let class_manager = self.class_manager_config.as_ref();
        let consensus_manager = self.consensus_manager_config.as_ref();
        let gateway = self.gateway_config.as_ref();
        let mempool = self.mempool_config.as_ref();
        let mempool_p2p = self.mempool_p2p_config.as_ref();
        let sierra_compiler = self.sierra_compiler_config.as_ref();
        let state_sync = self.state_sync_config.as_ref();
        let l1_events_scraper = self.l1_events_scraper_config.as_ref();
        let l1_gas_price_scraper = self.l1_gas_price_scraper_config.as_ref();

        // `chain_id`: shared by every component that touches storage, networking, or chain context.
        all_present_equal(
            "chain_id",
            &[
                batcher.map(|c| &c.static_config.block_builder_config.chain_info.chain_id),
                batcher.map(|c| &c.static_config.storage.db_config.chain_id),
                class_manager.map(|c| {
                    &c.static_config
                        .class_storage_config
                        .class_hash_storage_config
                        .db_config
                        .chain_id
                }),
                consensus_manager.map(|c| {
                    &c.consensus_manager_config.static_config.storage_config.db_config.chain_id
                }),
                consensus_manager.map(|c| &c.context_config.static_config.chain_id),
                consensus_manager.map(|c| &c.network_config.chain_id),
                gateway.map(|c| &c.static_config.chain_info.chain_id),
                l1_events_scraper.map(|c| &c.chain_id),
                l1_gas_price_scraper.map(|c| &c.chain_id),
                mempool_p2p.map(|c| &c.network_config.chain_id),
                state_sync.map(|c| &c.static_config.storage_config.db_config.chain_id),
                state_sync.and_then(|c| {
                    c.static_config.network_config.as_ref().map(|network| &network.chain_id)
                }),
                state_sync.map(|c| &c.static_config.rpc_config.chain_id),
            ],
        )?;

        // `eth_fee_token_address`: note the name asymmetry — state_sync calls it
        // `eth_fee_contract_address` but it is the same `ContractAddress` value.
        all_present_equal(
            "eth_fee_token_address",
            &[
                batcher.map(|c| {
                    &c.static_config
                        .block_builder_config
                        .chain_info
                        .fee_token_addresses
                        .eth_fee_token_address
                }),
                gateway
                    .map(|c| &c.static_config.chain_info.fee_token_addresses.eth_fee_token_address),
                state_sync
                    .map(|c| &c.static_config.rpc_config.execution_config.eth_fee_contract_address),
            ],
        )?;

        // `strk_fee_token_address`: same name asymmetry as `eth_fee_token_address`.
        all_present_equal(
            "strk_fee_token_address",
            &[
                batcher.map(|c| {
                    &c.static_config
                        .block_builder_config
                        .chain_info
                        .fee_token_addresses
                        .strk_fee_token_address
                }),
                gateway.map(|c| {
                    &c.static_config.chain_info.fee_token_addresses.strk_fee_token_address
                }),
                state_sync.map(|c| {
                    &c.static_config.rpc_config.execution_config.strk_fee_contract_address
                }),
            ],
        )?;

        // `recorder_url`: shared by the consensus cende client, the batcher pre-confirmed cende
        // client, and the mempool.
        all_present_equal(
            "recorder_url",
            &[
                consensus_manager.map(|c| &c.cende_config.recorder_url),
                batcher.map(|c| &c.static_config.pre_confirmed_cende_config.recorder_url),
                mempool.map(|c| &c.static_config.recorder_url),
            ],
        )?;

        // `native_classes_whitelist`: shared by the batcher and gateway dynamic configs.
        all_present_equal(
            "native_classes_whitelist",
            &[
                batcher.map(|c| &c.dynamic_config.native_classes_whitelist),
                gateway.map(|c| &c.dynamic_config.native_classes_whitelist),
            ],
        )?;

        // `validate_resource_bounds`: shared by the gateway stateful/stateless validators and the
        // mempool.
        all_present_equal(
            "validate_resource_bounds",
            &[
                gateway.map(|c| {
                    &c.static_config.stateful_tx_validator_config.validate_resource_bounds
                }),
                gateway.map(|c| {
                    &c.static_config.stateless_tx_validator_config.validate_resource_bounds
                }),
                mempool.map(|c| &c.static_config.validate_resource_bounds),
            ],
        )?;

        // `max_cpu_time`: the standalone sierra compiler and the batcher/gateway native compilers.
        all_present_equal(
            "max_cpu_time",
            &[
                sierra_compiler.map(|c| &c.max_cpu_time),
                batcher.map(|c| {
                    &c.static_config
                        .contract_class_manager_config
                        .native_compiler_config
                        .max_cpu_time
                }),
                gateway.map(|c| {
                    &c.static_config
                        .contract_class_manager_config
                        .native_compiler_config
                        .max_cpu_time
                }),
            ],
        )?;

        // `behavior_mode`: shared by the consensus context and the mempool.
        all_present_equal(
            "behavior_mode",
            &[
                consensus_manager.map(|c| &c.context_config.static_config.behavior_mode),
                mempool.map(|c| &c.static_config.behavior_mode),
            ],
        )?;

        // `versioned_constants_overrides`: shared by the batcher block builder and the gateway
        // stateful validator. Both sides are `Option`, compared by value.
        all_present_equal(
            "versioned_constants_overrides",
            &[
                batcher
                    .map(|c| &c.static_config.block_builder_config.versioned_constants_overrides),
                gateway.map(|c| {
                    &c.static_config.stateful_tx_validator_config.versioned_constants_overrides
                }),
            ],
        )?;

        // `revert_config`: shared by state_sync and the consensus manager.
        all_present_equal(
            "revert_config",
            &[
                state_sync.map(|c| &c.static_config.revert_config),
                consensus_manager.map(|c| &c.revert_config),
            ],
        )?;

        // `validation_only`: the always-present top-level flag is the source; the batcher's copy
        // (`batcher_config.static_config.validation_only`) is the lone target and is what actually
        // drives batcher behavior. They are read independently, so assert they agree.
        all_present_equal(
            "validation_only",
            &[Some(&self.validation_only), batcher.map(|c| &c.static_config.validation_only)],
        )?;

        Ok(())
    }

    /// Validates that when `validation_only=true`, all tx-ingestion components are disabled.
    fn validate_validation_only_config(&self) -> Result<(), ConfigError> {
        if !self.validation_only {
            return Ok(());
        }
        self.components.validate_tx_ingestion_components_disabled().map_err(|e| match e {
            ConfigError::ComponentConfigMismatch { component_config_mismatch } => {
                ConfigError::ComponentConfigMismatch {
                    component_config_mismatch: format!(
                        "{component_config_mismatch} when validation_only is true"
                    ),
                }
            }
            other => other,
        })?;
        // TODO(Asaf): Revert PR 14122 and require StorageScope::StateOnly here once
        // state_sync.get_block() works without transaction_metadata (the orchestrator needs it).
        Ok(())
    }
}

/// Asserts that all present values in `values` are equal, returning an error otherwise.
///
/// Only the `Some` entries participate: absent components contribute nothing, so a config that
/// owns just a subset of a pointer group still validates. `label` names the pointer group (e.g.
/// `"chain_id"`) and is surfaced in the error message. Values are compared by reference, since
/// they are borrowed out of `&self`'s component `Option`s and cannot be moved.
fn all_present_equal<T: PartialEq + std::fmt::Debug>(
    label: &str,
    values: &[Option<&T>],
) -> Result<(), ConfigError> {
    let present_values: Vec<&T> = values.iter().filter_map(|value| *value).collect();
    let Some((first_value, rest_values)) = present_values.split_first() else {
        return Ok(());
    };
    if let Some(mismatched_value) = rest_values.iter().find(|value| *value != first_value) {
        return Err(ConfigError::ComponentConfigMismatch {
            component_config_mismatch: format!(
                "{label} values mismatch across components: {first_value:?} != \
                 {mismatched_value:?}"
            ),
        });
    }
    Ok(())
}

/// The command line interface of this node.
pub fn node_command() -> Command {
    Command::new("Sequencer")
        .version(VERSION_FULL)
        .about("A Starknet sequencer node written in Rust.")
}
