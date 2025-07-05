use std::path::Path;

use apollo_config::dumping::{combine_config_map_and_pointers, Pointers, SerializeConfig};
use apollo_config::CONFIG_FILE_ARG;
use apollo_infra_utils::dumping::serialize_to_file;
use apollo_monitoring_endpoint::config::MonitoringEndpointConfig;
use serde_json::{to_value, Map, Value};
use tracing::error;
use validator::ValidationError;

use crate::config::component_config::ComponentConfig;
use crate::config::definitions::ConfigPointersMap;
use crate::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
    POINTER_TARGET_VALUE,
};
use crate::utils::load_and_validate_config;

pub(crate) fn create_validation_error(
    error_msg: String,
    validate_code: &'static str,
    validate_error_msg: &'static str,
) -> ValidationError {
    error!(error_msg);
    let mut error = ValidationError::new(validate_code);
    error.message = Some(validate_error_msg.into());
    error
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

// TODO(Nadin): Consider adding methods to ConfigPointers to encapsulate related functionality.
fn validate_all_pointer_targets_set(preset: Value) -> Result<(), ValidationError> {
    if let Some(preset_map) = preset.as_object() {
        for (key, value) in preset_map {
            if value == POINTER_TARGET_VALUE {
                return Err(create_validation_error(
                    format!("Pointer target not set for key: '{key}'"),
                    "pointer_target_not_set",
                    "Pointer target not set",
                ));
            }
        }
        Ok(())
    } else {
        Err(create_validation_error(
            "Preset must be an object".to_string(),
            "invalid_preset_format",
            "Preset is not a valid object",
        ))
    }
}

pub struct BaseAppConfigOverride {
    component_config: ComponentConfig,
    monitoring_endpoint_config: MonitoringEndpointConfig,
}

impl BaseAppConfigOverride {
    pub fn new(
        component_config: ComponentConfig,
        monitoring_endpoint_config: MonitoringEndpointConfig,
    ) -> Self {
        Self { component_config, monitoring_endpoint_config }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentBaseAppConfig {
    pub config: SequencerNodeConfig,
    config_pointers_map: ConfigPointersMap,
    non_pointer_params: Pointers,
}

impl DeploymentBaseAppConfig {
    pub fn new(
        config: SequencerNodeConfig,
        config_pointers_map: ConfigPointersMap,
        non_pointer_params: Pointers,
    ) -> Self {
        Self { config, config_pointers_map, non_pointer_params }
    }

    pub fn get_config(&self) -> &SequencerNodeConfig {
        &self.config
    }

    pub fn get_config_pointers_map(&self) -> &ConfigPointersMap {
        &self.config_pointers_map
    }

    pub fn modify_config<F>(&mut self, modify_config_fn: F)
    where
        F: Fn(&mut SequencerNodeConfig),
    {
        modify_config_fn(&mut self.config);
    }

    pub fn modify_config_pointers<F>(&mut self, modify_config_pointers_fn: F)
    where
        F: Fn(&mut ConfigPointersMap),
    {
        modify_config_pointers_fn(&mut self.config_pointers_map);
    }

    pub fn override_base_app_config(&mut self, base_app_config_override: BaseAppConfigOverride) {
        self.config.components = base_app_config_override.component_config;
        self.config.monitoring_endpoint_config =
            base_app_config_override.monitoring_endpoint_config;
    }

    pub fn as_value(&self) -> Value {
        // Create the entire mapping of the config and the pointers, without the required params.
        let config_as_map = combine_config_map_and_pointers(
            self.config.dump(),
            // TODO(Tsabary): avoid the cloning here
            &self.config_pointers_map.clone().into(),
            &self.non_pointer_params,
        )
        .unwrap();

        // Extract only the required fields from the config map.
        let preset = config_to_preset(&config_as_map);
        validate_all_pointer_targets_set(preset.clone()).expect("Pointer target not set");
        preset
    }

    // TODO(Tsabary): unify path types throughout.
    pub fn dump_config_file(&self, config_path: &Path) {
        let value = self.as_value();
        serialize_to_file(
            value,
            config_path.to_str().expect("Should be able to convert path to string"),
        );
    }
}

pub fn get_deployment_from_config_path(config_path: &str) -> DeploymentBaseAppConfig {
    // TODO(Nadin): simplify this by using only config_path and removing the extra strings.
    let config = load_and_validate_config(vec![
        "deployment_from_config_path".to_string(),
        CONFIG_FILE_ARG.to_string(),
        config_path.to_string(),
    ])
    .unwrap();

    let mut config_pointers_map = ConfigPointersMap::new(CONFIG_POINTERS.clone());

    config_pointers_map.change_target_value(
        "chain_id",
        to_value(config.batcher_config.block_builder_config.chain_info.chain_id.clone())
            .expect("Failed to serialize ChainId"),
    );
    config_pointers_map.change_target_value(
        "eth_fee_token_address",
        to_value(
            config
                .batcher_config
                .block_builder_config
                .chain_info
                .fee_token_addresses
                .eth_fee_token_address,
        )
        .expect("Failed to serialize ContractAddress"),
    );
    config_pointers_map.change_target_value(
        "strk_fee_token_address",
        to_value(
            config
                .batcher_config
                .block_builder_config
                .chain_info
                .fee_token_addresses
                .strk_fee_token_address,
        )
        .expect("Failed to serialize ContractAddress"),
    );
    config_pointers_map.change_target_value(
        "validator_id",
        to_value(config.consensus_manager_config.consensus_config.validator_id)
            .expect("Failed to serialize ContractAddress"),
    );
    config_pointers_map.change_target_value(
        "recorder_url",
        to_value(config.consensus_manager_config.cende_config.recorder_url.clone())
            .expect("Failed to serialize Url"),
    );
    config_pointers_map.change_target_value(
        "starknet_url",
        to_value(config.state_sync_config.rpc_config.starknet_url.clone())
            .expect("Failed to serialize starknet_url"),
    );

    DeploymentBaseAppConfig::new(config, config_pointers_map, CONFIG_NON_POINTERS_WHITELIST.clone())
}
