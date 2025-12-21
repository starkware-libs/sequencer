use std::collections::BTreeMap;
use std::path::PathBuf;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Configuration for the proof manager.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct ProofManagerConfig {
    pub persistent_root: PathBuf,
}
impl Default for ProofManagerConfig {
    fn default() -> Self {
        Self { persistent_root: "/data/proofs".into() }
    }
}
impl SerializeConfig for ProofManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "persistent_root",
            &self.persistent_root,
            "Persistent root for proof storage.",
            ParamPrivacyInput::Public,
        )])
    }
}
