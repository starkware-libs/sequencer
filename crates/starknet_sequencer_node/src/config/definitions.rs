use std::collections::HashMap;

use papyrus_config::dumping::{ConfigPointers, Pointers};
use papyrus_config::{ParamPath, SerializedContent, SerializedParam};
use serde_json::Value;

#[derive(Debug, Clone)]
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
}

impl From<ConfigPointersMap> for ConfigPointers {
    fn from(config_pointers_map: ConfigPointersMap) -> Self {
        config_pointers_map.0.into_iter().map(|(k, (v, p))| ((k, v), p)).collect()
    }
}
