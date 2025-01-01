use std::collections::{BTreeMap, HashSet};

use papyrus_config::dumping::{combine_config_map_and_pointers, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_protobuf::consensus::DEFAULT_VALIDATOR_ID;
use serde::Serialize;
use serde_json::{Map, Value};
use starknet_api::core::{ChainId, ContractAddress};
use url::Url;

use crate::config::node_config::node_command;

/// Required parameters utility struct.
#[derive(Serialize)]
pub struct RequiredParams {
    pub chain_id: ChainId,
    pub eth_fee_token_address: ContractAddress,
    pub strk_fee_token_address: ContractAddress,
    pub validator_id: ContractAddress,
    pub recorder_url: Url,
}

impl SerializeConfig for RequiredParams {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let members = BTreeMap::from_iter([
            ser_param("chain_id", &self.chain_id, "Placeholder.", ParamPrivacyInput::Public),
            ser_param(
                "eth_fee_token_address",
                &self.eth_fee_token_address,
                "Placeholder.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "strk_fee_token_address",
                &self.strk_fee_token_address,
                "Placeholder.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "validator_id",
                &self.validator_id,
                "Placeholder.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "recorder_url",
                &self.recorder_url,
                "Placeholder.",
                ParamPrivacyInput::Public,
            ),
        ]);
        vec![members].into_iter().flatten().collect()
    }
}

impl RequiredParams {
    pub fn create_for_testing() -> Self {
        Self {
            chain_id: ChainId::create_for_testing(),
            eth_fee_token_address: ContractAddress::from(2_u128),
            strk_fee_token_address: ContractAddress::from(3_u128),
            validator_id: ContractAddress::from(DEFAULT_VALIDATOR_ID),
            recorder_url: Url::parse("https://recorder_url").expect("The URL is valid"),
        }
    }

    pub fn as_json(&self) -> Value {
        let config_as_map = combine_config_map_and_pointers(self.dump(), &vec![], &HashSet::new())
            .expect("Failed to combine config map.");
        config_to_preset(&config_as_map)
    }

    pub fn cli_args(&self) -> Vec<String> {
        let self_as_json = self.as_json();
        if let Value::Object(map) = self_as_json {
            map.iter()
                .flat_map(|(key, value)| {
                    vec![format!("--{}", key), value.to_string().trim_matches('"').to_string()]
                })
                .collect()
        } else {
            panic!("Required params are not a JSON map object: {:?}", self_as_json);
        }
    }

    pub fn field_names(&self) -> HashSet<String> {
        let self_as_json = self.as_json();
        if let Value::Object(map) = self_as_json {
            map.keys().cloned().collect()
        } else {
            panic!("Required params are not a JSON map object: {:?}", self_as_json);
        }
    }
}

// Creates a vector of strings with the command name and required parameters that can be used as
// arguments to load a config.
pub fn create_test_config_load_args(required_params: RequiredParams) -> Vec<String> {
    let mut cli_args = vec![node_command().to_string()];
    cli_args.extend(required_params.cli_args());
    cli_args
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
        panic!("Config map is not a JSON object: {:?}", config_map);
    }
}
