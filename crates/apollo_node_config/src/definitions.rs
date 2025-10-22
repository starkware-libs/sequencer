use std::collections::HashMap;

use apollo_config::dumping::{ConfigPointers, Pointers};
use apollo_config::{ParamPath, SerializedContent, SerializedParam};
#[cfg(any(feature = "testing", test))]
use serde_json::to_value;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct ConfigPointersMap(HashMap<ParamPath, (SerializedParam, Pointers)>);

impl ConfigPointersMap {
    pub fn new(config_pointers: ConfigPointers) -> Self {
        ConfigPointersMap(config_pointers.into_iter().map(|((k, v), p)| (k, (v, p))).collect())
    }

    pub fn change_target_value(&mut self, target: &str, value: Value) {
        assert!(self.0.contains_key(target));
        self.0.entry(target.to_owned()).and_modify(|(param, _)| {
            param.content = SerializedContent::DefaultValue(value);
        });
    }

    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing(config_pointers: ConfigPointers) -> Self {
        let mut config_pointers_map = Self::new(config_pointers);

        // Set all required pointer targets with sensible testing defaults
        config_pointers_map.change_target_value(
            "chain_id",
            to_value("SN_SEPOLIA").expect("Failed to serialize ChainId"),
        );
        config_pointers_map.change_target_value(
            "validator_id",
            to_value("0x64").expect("Failed to serialize validator_id"),
        );
        config_pointers_map.change_target_value(
            "eth_fee_token_address",
            to_value("0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7")
                .expect("Failed to serialize eth_fee_token_address"),
        );
        config_pointers_map.change_target_value(
            "strk_fee_token_address",
            to_value("0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d")
                .expect("Failed to serialize strk_fee_token_address"),
        );
        config_pointers_map.change_target_value(
            "recorder_url",
            to_value("http://localhost:8080").expect("Failed to serialize recorder_url"),
        );
        config_pointers_map.change_target_value(
            "starknet_url",
            to_value("http://localhost:8081").expect("Failed to serialize starknet_url"),
        );

        config_pointers_map
    }
}

impl From<ConfigPointersMap> for ConfigPointers {
    fn from(config_pointers_map: ConfigPointersMap) -> Self {
        config_pointers_map.0.into_iter().map(|(k, (v, p))| ((k, v), p)).collect()
    }
}

// TODO(Nadin/Tsabary): reduce visibility throughout this module, and consider unifying with the
// `utils` module.

#[derive(Debug, Clone, Copy)]
pub enum ConfigExpectation {
    Redundant,
    Required,
}

#[derive(Debug, Clone, Copy)]
pub enum ConfigPresence {
    Absent,
    Present,
}

impl<T> From<&Option<T>> for ConfigPresence {
    fn from(opt: &Option<T>) -> Self {
        if opt.is_some() { ConfigPresence::Present } else { ConfigPresence::Absent }
    }
}
