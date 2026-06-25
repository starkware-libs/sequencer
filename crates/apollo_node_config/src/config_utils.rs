use std::collections::BTreeSet;
use std::fs::File;
use std::path::Path;

use apollo_config::presentation::get_config_presentation;
use apollo_config::{ConfigError, ParamPath};
use apollo_infra_utils::dumping::serialize_to_file;
use apollo_infra_utils::path::resolve_project_relative_path;
use serde_json::Value;
use tracing::{error, info};

use crate::node_config::{SequencerNodeConfig, CONFIG_SECRETS_SCHEMA_PATH};

/// Returns the set of all non-pointer private parameters and all pointer target parameters pointed
/// by private parameters, as committed in the secrets schema file (`CONFIG_SECRETS_SCHEMA_PATH`).
///
/// The committed file is hand-maintained. The `secrets_schema_contains_all_default_redacted_fields`
/// test guards (best-effort) that every default-redacted `Sensitive` field is present in it.
pub fn private_parameters() -> BTreeSet<ParamPath> {
    let secrets_schema_path = &resolve_project_relative_path(CONFIG_SECRETS_SCHEMA_PATH).unwrap();
    let secrets_schema_file = File::open(secrets_schema_path).unwrap();
    serde_json::from_reader(secrets_schema_file).unwrap()
}

// TODO(Nadin/Tsabary): `DeploymentBaseAppConfig` is only used in tests, and should be marked as
// such.
#[derive(Debug, Clone, Default)]
pub struct DeploymentBaseAppConfig {
    pub config: SequencerNodeConfig,
}

impl DeploymentBaseAppConfig {
    pub fn new(config: SequencerNodeConfig) -> Self {
        Self { config }
    }

    pub fn get_config(&self) -> &SequencerNodeConfig {
        &self.config
    }

    pub fn modify_config<F>(&mut self, modify_config_fn: F)
    where
        F: Fn(&mut SequencerNodeConfig),
    {
        modify_config_fn(&mut self.config);
    }

    /// Returns the nested config as JSON, matching the `SequencerNodeConfig` field hierarchy.
    /// This is the artifact consumed by the native config loader (as the base config).
    pub fn as_native_value(&self) -> Value {
        serde_json::to_value(&self.config).expect("Should be able to serialize config to value")
    }

    /// Dumps the nested native base config (see `as_native_value`) to `config_path`.
    pub fn dump_native_config_file(&self, config_path: &Path) {
        let value = self.as_native_value();
        serialize_to_file(
            &value,
            config_path.to_str().expect("Should be able to convert path to string"),
        );
    }
}

pub fn load_and_validate_config(
    args: Vec<String>,
    log_enabled: bool,
) -> Result<SequencerNodeConfig, ConfigError> {
    let config_load_result = SequencerNodeConfig::load_and_process(args);
    if let Err(error) = config_load_result {
        error!("Failed loading configuration: {error}");
        return Err(error);
    }
    let loaded_config = config_load_result.unwrap();

    if log_enabled {
        info!("Finished loading configuration.");
    }

    let config_validation_result = loaded_config.validate_node_config();
    if let Err(error) = config_validation_result {
        error!("Config validation failed: {error}");
        return Err(error);
    }

    if log_enabled {
        info!("Finished validating configuration.");
        info!("Config map:");
        info!(
            "{:#?}",
            get_config_presentation(&loaded_config, false, &private_parameters())
                .expect("Should be able to get representation.")
        );
        info!("Finished dumping configuration.");
    }

    Ok(loaded_config)
}

/// Overwrites every present target of each multi-target pointer group with a single,
/// consistent value, mirroring what pointer resolution did at load time. This lets a config
/// assembled directly from `SequencerNodeConfig::default()` satisfy the cross-component equality
/// invariant enforced by `validate_node_config`.
#[cfg(any(feature = "testing", test))]
pub fn normalize_pointer_groups(config: &mut SequencerNodeConfig) {
    use apollo_config::behavior_mode::BehaviorMode;
    use apollo_reverts::RevertConfig;
    use blockifier::blockifier::config::NativeClassesWhitelist;
    use starknet_api::core::{ChainId, ContractAddress};

    let chain_id = ChainId::Mainnet;
    let eth_fee_token_address = ContractAddress::from(1u128);
    let strk_fee_token_address = ContractAddress::from(2u128);
    let max_cpu_time: u64 = 600;

    config.validation_only = false;
    if let Some(sierra_compiler) = config.sierra_compiler_config.as_mut() {
        sierra_compiler.max_cpu_time = max_cpu_time;
    }
    if let Some(batcher) = config.batcher_config.as_mut() {
        let static_config = &mut batcher.static_config;
        static_config.block_builder_config.chain_info.chain_id = chain_id.clone();
        static_config.storage.db_config.chain_id = chain_id.clone();
        let fee_token_addresses =
            &mut static_config.block_builder_config.chain_info.fee_token_addresses;
        fee_token_addresses.eth_fee_token_address = eth_fee_token_address;
        fee_token_addresses.strk_fee_token_address = strk_fee_token_address;
        static_config.contract_class_manager_config.native_compiler_config.max_cpu_time =
            max_cpu_time;
        static_config.pre_confirmed_cende_config.recorder_url =
            "https://recorder_url".parse().unwrap();
        static_config.block_builder_config.versioned_constants_overrides = None;
        static_config.validation_only = false;
        batcher.dynamic_config.native_classes_whitelist = NativeClassesWhitelist::All;
    }
    if let Some(class_manager) = config.class_manager_config.as_mut() {
        class_manager
            .static_config
            .class_storage_config
            .class_hash_storage_config
            .db_config
            .chain_id = chain_id.clone();
    }
    if let Some(consensus_manager) = config.consensus_manager_config.as_mut() {
        consensus_manager
            .consensus_manager_config
            .static_config
            .storage_config
            .db_config
            .chain_id = chain_id.clone();
        consensus_manager.context_config.static_config.chain_id = chain_id.clone();
        consensus_manager.network_config.chain_id = chain_id.clone();
        consensus_manager.context_config.static_config.behavior_mode = BehaviorMode::Starknet;
        consensus_manager.cende_config.recorder_url = "https://recorder_url".parse().unwrap();
        consensus_manager.revert_config = RevertConfig::default();
    }
    if let Some(gateway) = config.gateway_config.as_mut() {
        gateway.static_config.chain_info.chain_id = chain_id.clone();
        let fee_token_addresses = &mut gateway.static_config.chain_info.fee_token_addresses;
        fee_token_addresses.eth_fee_token_address = eth_fee_token_address;
        fee_token_addresses.strk_fee_token_address = strk_fee_token_address;
        gateway.static_config.contract_class_manager_config.native_compiler_config.max_cpu_time =
            max_cpu_time;
        gateway.static_config.stateful_tx_validator_config.validate_resource_bounds = true;
        gateway.static_config.stateless_tx_validator_config.validate_resource_bounds = true;
        gateway.static_config.stateful_tx_validator_config.versioned_constants_overrides = None;
        gateway.dynamic_config.native_classes_whitelist = NativeClassesWhitelist::All;
    }
    if let Some(l1_events_scraper) = config.l1_events_scraper_config.as_mut() {
        l1_events_scraper.chain_id = chain_id.clone();
    }
    if let Some(l1_gas_price_scraper) = config.l1_gas_price_scraper_config.as_mut() {
        l1_gas_price_scraper.chain_id = chain_id.clone();
    }
    if let Some(mempool) = config.mempool_config.as_mut() {
        mempool.static_config.recorder_url = "https://recorder_url".parse().unwrap();
        mempool.static_config.validate_resource_bounds = true;
        mempool.static_config.behavior_mode = BehaviorMode::Starknet;
    }
    if let Some(mempool_p2p) = config.mempool_p2p_config.as_mut() {
        mempool_p2p.network_config.chain_id = chain_id.clone();
    }
    if let Some(state_sync) = config.state_sync_config.as_mut() {
        let static_config = &mut state_sync.static_config;
        static_config.storage_config.db_config.chain_id = chain_id.clone();
        if let Some(network_config) = static_config.network_config.as_mut() {
            network_config.chain_id = chain_id.clone();
        }
        static_config.rpc_config.chain_id = chain_id.clone();
        static_config.rpc_config.execution_config.eth_fee_contract_address = eth_fee_token_address;
        static_config.rpc_config.execution_config.strk_fee_contract_address =
            strk_fee_token_address;
        static_config.revert_config = RevertConfig::default();
    }
}
