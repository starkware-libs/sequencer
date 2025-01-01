use std::collections::HashSet;
use std::vec::Vec; // Used by #[gen_field_names_fn].

use papyrus_protobuf::consensus::DEFAULT_VALIDATOR_ID;
use serde::Serialize;
use serde_json::{to_value, Value};
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
        to_value(self).unwrap()
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
