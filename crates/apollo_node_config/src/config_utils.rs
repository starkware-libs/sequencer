use std::collections::{BTreeSet, HashSet};
use std::fs::File;
use std::path::Path;

use apollo_config::presentation::get_config_presentation;
use apollo_config::{ConfigError, ParamPath, FIELD_SEPARATOR, IS_NONE_MARK};
use apollo_infra_utils::dumping::serialize_to_file;
use apollo_infra_utils::path::resolve_project_relative_path;
use serde_json::{Map, Value};
use tracing::{error, info};

use crate::node_config::{SequencerNodeConfig, CONFIG_SECRETS_SCHEMA_PATH};

/// Returns the set of all non-pointer private parameters and all pointer target parameters pointed
/// by private parameters, as committed in the secrets schema file (`CONFIG_SECRETS_SCHEMA_PATH`).
///
/// The committed file is exactly this set serialized; the `default_config_file_is_up_to_date` test
/// guards that it stays in sync with the config derivation.
pub fn private_parameters() -> BTreeSet<ParamPath> {
    let secrets_schema_path = &resolve_project_relative_path(CONFIG_SECRETS_SCHEMA_PATH).unwrap();
    let secrets_schema_file = File::open(secrets_schema_path).unwrap();
    serde_json::from_reader(secrets_schema_file).unwrap()
}

/// Transforms a nested JSON dictionary object into a simplified JSON dictionary object by
/// extracting specific values from the inner dictionaries.
///
/// # Parameters
/// - `config_map`: A reference to a `serde_json::Value` that must be a JSON dictionary object. Each
///   key in the object maps to another JSON dictionary object.
///
/// # Returns
/// - A `serde_json::Value` dictionary object where:
///   - Each key is preserved from the top-level dictionary.
///   - Each value corresponds to the `"value"` field of the nested JSON dictionary under the
///     original key.
///
/// # Panics
/// This function panics if the provided `config_map` is not a JSON dictionary object.
pub fn config_to_preset(config_map: &Value) -> Value {
    // Ensure the config_map is a JSON object.
    if let Value::Object(map) = config_map {
        let mut result = Map::new();

        for (key, value) in map {
            if let Value::Object(inner_map) = value {
                // Extract the value.
                if let Some(inner_value) = inner_map.get("value") {
                    // Add it to the result map
                    result.insert(key.clone(), inner_value.clone());
                }
            }
        }

        // Return the transformed result as a JSON object.
        Value::Object(result)
    } else {
        panic!("Config map is not a JSON object: {config_map:?}");
    }
}

/// Keep "{prefix}.#is_none": true, remove all other keys that begin with "{prefix}" (including
/// the bare prefix).
pub fn prune_by_is_none(mut v: Value) -> Value {
    let obj: &mut Map<String, Value> =
        v.as_object_mut().expect("prune_by_is_none: expected a JSON object");

    // Find optional parameter paths which are unset
    let is_none_suffix = format!("{FIELD_SEPARATOR}{IS_NONE_MARK}");
    let mut unset_optional_param_paths: HashSet<String> = HashSet::new();

    for (k, val) in obj.iter() {
        if let Some(prefix) = k.strip_suffix(&is_none_suffix) {
            if val.as_bool() == Some(true) {
                unset_optional_param_paths.insert(prefix.to_string());
            }
        }
    }

    // Remove keys that begin with any such prefix (including the bare prefix), except the
    // "#is_none" flag itself
    obj.retain(|k, _| {
        if let Some(p) = unset_optional_param_paths.iter().find(|p| k.starts_with(&***p)) {
            // keep only the "{prefix}.#is_none" key
            k == &format!("{p}{FIELD_SEPARATOR}{IS_NONE_MARK}")
        } else {
            true
        }
    });

    v
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
            get_config_presentation::<SequencerNodeConfig>(&loaded_config, false)
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
