use std::collections::HashMap;

use apollo_config::dumping::{ConfigPointers, Pointers};
use apollo_config::{ParamPath, SerializedContent, SerializedParam};
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
}

impl From<ConfigPointersMap> for ConfigPointers {
    fn from(config_pointers_map: ConfigPointersMap) -> Self {
        config_pointers_map.0.into_iter().map(|(k, (v, p))| ((k, v), p)).collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ConfigExpectation {
    Redundant,
    Required,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ConfigPresence {
    Absent,
    Present,
}

impl<T> From<&Option<T>> for ConfigPresence {
    fn from(opt: &Option<T>) -> Self {
        if opt.is_some() { ConfigPresence::Present } else { ConfigPresence::Absent }
    }
}
